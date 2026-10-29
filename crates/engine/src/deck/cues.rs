//! Quantize, hot cues, beat jump and slip.

use opendeck_contracts::cue::{FrameRegion, HotCueIndex};
use opendeck_contracts::{Beats, EngineEvent, Frames, Seconds};

use super::{Deck, EventSink, Loop, Pending, SLIP_HOTCUE, SLIP_ROLL, Transport};

impl Deck {
    /// Source frames per beat at original speed, if the track has a grid.
    pub(super) fn beat_len(&self) -> Option<f64> {
        self.grid.map(|g| 60.0 / g.bpm().0 * self.sample_rate)
    }

    /// Snaps `p` (source frames) to the nearest beat when quantize is on and
    /// the track has a grid. Otherwise returns it unchanged.
    pub(super) fn quantize_pos(&self, p: f64) -> f64 {
        let snapped = match self.grid {
            Some(grid) if self.quantize => {
                grid.nearest_beat(Seconds(p / self.sample_rate)).0 * self.sample_rate
            }
            _ => p,
        };
        snapped.clamp(0.0, self.len_frames())
    }

    /// First beat strictly after `p`, in source frames.
    pub(super) fn next_beat_after(&self, p: f64) -> Option<f64> {
        let grid = self.grid?;
        let beat = grid.beat_at(Seconds(p / self.sample_rate)).0.floor() + 1.0;
        Some(grid.time_of(Beats(beat)).0 * self.sample_rate)
    }

    /// Smallest loop, source frames: long enough for the click-free wrap.
    pub(super) fn min_loop(&self) -> f64 {
        (self.declick * 3) as f64
    }

    /// Goes to `to` as a performance jump: on the next beat when quantize is
    /// on, the deck is playing and has a grid, at once otherwise.
    pub(super) fn go_to(&mut self, to: f64) {
        let beat = if self.quantize && self.transport == Transport::Playing {
            self.next_beat_after(self.pos)
        } else {
            None
        };
        match beat {
            Some(at) if self.scratch.is_none() => self.pending = Some(Pending { at, to }),
            _ => {
                self.pending = None;
                self.relocate(to);
            }
        }
    }

    // ----- Hot cues -----

    pub(crate) fn set_hot_cue(&mut self, index: HotCueIndex, cue: Option<FrameRegion>) {
        if let Some(slot) = self.hot.get_mut(index.index()) {
            *slot = cue;
        }
    }

    /// Hot cue pad edge.
    ///
    /// - Empty slot, press: store the playhead (snapped when quantize is on).
    /// - Playing, press: jump to the cue (on the next beat when quantize is
    ///   on). A hot loop also starts looping. With slip on, release returns to
    ///   where playback would have been.
    /// - Stopped, press: preview from the cue until release, then return.
    pub(crate) fn hot_cue(&mut self, index: HotCueIndex, pressed: bool, sink: &mut EventSink) {
        let i = index.index();
        if self.track.is_none() {
            return;
        }
        if !pressed {
            if self.transport == Transport::CuePreview && self.preview_hot == Some(i) {
                self.stop_at(self.preview_return);
            }
            if self.slip_hold & SLIP_HOTCUE != 0 {
                self.end_slip(SLIP_HOTCUE);
            }
            return;
        }
        let Some(slot) = self.hot.get(i).copied() else {
            return;
        };
        let Some(cue) = slot else {
            let start = self.quantize_pos(self.pos).round();
            let region = FrameRegion {
                start: Frames(start as u64),
                end: None,
            };
            if let Some(entry) = self.hot.get_mut(i) {
                *entry = Some(region);
                sink.push(EngineEvent::HotCueSet {
                    deck: self.id,
                    index,
                    cue: region,
                });
            }
            return;
        };
        let start = (cue.start.0 as f64).min(self.len_frames());
        if self.transport == Transport::Playing {
            if self.slip {
                self.begin_slip(SLIP_HOTCUE);
            }
            if let Some(end) = cue.end {
                let end = end.0 as f64;
                if end > start + self.min_loop() {
                    self.loop_stored = Some(Loop { start, end });
                    self.loop_active = true;
                    self.loop_in = None;
                }
            }
            self.go_to(start);
        } else {
            let was_stopped = self.transport == Transport::Stopped;
            self.pending = None;
            if was_stopped {
                self.pos = start;
            } else {
                self.move_to(start);
            }
            self.preview_return = start;
            self.preview_hot = Some(i);
            self.transport = Transport::CuePreview;
            if was_stopped {
                self.begin_audible();
            }
        }
    }

    // ----- Beat jump -----

    /// Jumps by `beats` (signed), keeping the beat phase. A loop moves with the
    /// playhead. Needs a beat grid.
    pub(crate) fn beat_jump(&mut self, beats: Beats) {
        let Some(beat_len) = self.beat_len() else {
            return;
        };
        if self.track.is_none() || !beats.0.is_finite() {
            return;
        }
        let len = self.len_frames();
        let target = (self.pos + beats.0 * beat_len).clamp(0.0, len);
        let applied = target - self.pos;
        if applied.abs() < 0.5 {
            return;
        }
        if let Some(l) = self.loop_stored.filter(|_| self.loop_active) {
            let moved = Loop {
                start: l.start + applied,
                end: l.end + applied,
            };
            if moved.start < 0.0 || moved.end > len {
                return;
            }
            self.loop_stored = Some(moved);
        }
        self.pending = None;
        self.relocate(target);
    }

    // ----- Slip -----

    pub(crate) fn set_slip(&mut self, on: bool) {
        self.slip = on;
        if !on {
            self.slip_hold &= SLIP_ROLL;
            if self.slip_hold == 0 {
                self.slip_pos = None;
            }
        }
    }

    /// Starts the shadow playhead for `reason` if playing.
    pub(super) fn begin_slip(&mut self, reason: u8) {
        if self.transport != Transport::Playing {
            return;
        }
        if self.slip_pos.is_none() {
            self.slip_pos = Some(self.pos);
        }
        self.slip_hold |= reason;
    }

    /// Ends one reason for slip. When the last one ends the playhead jumps to
    /// the shadow. Returns true when it did.
    pub(super) fn end_slip(&mut self, reason: u8) -> bool {
        self.slip_hold &= !reason;
        if self.slip_hold != 0 {
            return false;
        }
        match self.slip_pos.take() {
            Some(p) if self.transport == Transport::Playing => {
                self.pending = None;
                self.relocate(p);
                true
            }
            _ => false,
        }
    }
}
