//! Loop in/out, auto loop, loop roll, loop resize.

use opendeck_contracts::cue::FrameRegion;
use opendeck_contracts::{Beats, EngineEvent, Frames};

use super::{Deck, EventSink, Loop, RollSaved, SLIP_LOOP, SLIP_ROLL, Transport};

impl Deck {
    /// Builds a valid loop: at least [`Deck::min_loop`] long, inside the track.
    fn make_loop(&self, start: f64, end: f64) -> Option<Loop> {
        if !(start.is_finite() && end.is_finite()) {
            return None;
        }
        let start = start.max(0.0);
        let end = end.max(start + self.min_loop());
        (end <= self.len_frames()).then_some(Loop { start, end })
    }

    fn announce_loop(&self, l: Loop, sink: &mut EventSink) {
        sink.push(EngineEvent::LoopSet {
            deck: self.id,
            region: FrameRegion {
                start: Frames(l.start.round() as u64),
                end: Some(Frames(l.end.round() as u64)),
            },
        });
    }

    /// Makes `l` the active loop. A playhead already past its end is folded
    /// back into it, keeping its offset from the loop start modulo the length.
    fn enter_loop(&mut self, l: Loop) {
        self.loop_stored = Some(l);
        self.loop_active = true;
        self.loop_in = None;
        if self.pos >= l.end {
            self.relocate(l.start + (self.pos - l.start).rem_euclid(l.len()));
        }
    }

    /// Leaves the loop. With slip the playhead jumps to the shadow; with key
    /// lock the stretcher is re-aligned to the plain stream.
    fn leave_loop(&mut self) {
        self.loop_active = false;
        let jumped = self.end_slip(SLIP_LOOP);
        if !jumped && self.key_lock && self.audible() && self.scratch.is_none() {
            self.move_to(self.pos);
        }
    }

    /// LOOP IN: remember the playhead, or move the start of the active loop.
    pub(crate) fn loop_in(&mut self) {
        if self.track.is_none() {
            return;
        }
        let p = self.quantize_pos(self.pos);
        if self.loop_active {
            if let Some(l) = self.loop_stored
                && p + self.min_loop() <= l.end
            {
                self.loop_stored = Some(Loop {
                    start: p,
                    end: l.end,
                });
            }
            return;
        }
        self.loop_in = Some(p);
    }

    /// LOOP OUT: close the loop opened by LOOP IN and start looping, or move
    /// the end of the active loop.
    pub(crate) fn loop_out(&mut self, sink: &mut EventSink) {
        if self.track.is_none() {
            return;
        }
        let p = self.quantize_pos(self.pos);
        if self.loop_active {
            if let Some(l) = self.loop_stored
                && p >= l.start + self.min_loop()
            {
                let moved = Loop {
                    start: l.start,
                    end: p,
                };
                self.loop_stored = Some(moved);
                if self.pos >= p {
                    self.relocate(moved.start + (self.pos - moved.start).rem_euclid(moved.len()));
                }
                self.announce_loop(moved, sink);
            }
            return;
        }
        let Some(start) = self.loop_in.take() else {
            return;
        };
        let Some(l) = self.make_loop(start, p) else {
            return;
        };
        if self.slip {
            self.begin_slip(SLIP_LOOP);
        }
        self.enter_loop(l);
        self.announce_loop(l, sink);
    }

    /// Exit the active loop, or re-enter the last one from its start.
    pub(crate) fn loop_toggle(&mut self) {
        if self.track.is_none() {
            return;
        }
        if self.loop_active {
            self.leave_loop();
        } else if let Some(l) = self.loop_stored {
            if self.slip {
                self.begin_slip(SLIP_LOOP);
            }
            self.loop_active = true;
            self.pending = None;
            self.relocate(l.start);
        }
    }

    /// Beat loop of `length` from the playhead (snapped when quantize is on).
    /// Needs a beat grid.
    pub(crate) fn auto_loop(&mut self, length: Beats, sink: &mut EventSink) {
        let Some(beat_len) = self.beat_len() else {
            return;
        };
        if self.track.is_none() || !(length.0.is_finite() && length.0 > 0.0) {
            return;
        }
        let start = self.quantize_pos(self.pos);
        let Some(l) = self.make_loop(start, start + length.0 * beat_len) else {
            return;
        };
        if self.slip && !self.loop_active {
            self.begin_slip(SLIP_LOOP);
        }
        self.enter_loop(l);
        self.announce_loop(l, sink);
    }

    /// Loop roll: loop while held, then resume where playback would have been.
    /// The roll starts on the grid multiple of `length` at or before the
    /// playhead when quantize is on. Needs a beat grid and a playing deck.
    pub(crate) fn loop_roll(&mut self, length: Beats, active: bool) {
        if !active {
            self.end_roll();
            return;
        }
        let (Some(grid), Some(beat_len)) = (self.grid, self.beat_len()) else {
            return;
        };
        if self.track.is_none()
            || self.transport != Transport::Playing
            || !(length.0.is_finite() && length.0 > 0.0)
        {
            return;
        }
        let start = if self.quantize {
            let beat = grid
                .beat_at(opendeck_contracts::Seconds(self.pos / self.sample_rate))
                .0;
            let on_grid = (beat / length.0).floor() * length.0;
            (grid.time_of(Beats(on_grid)).0 * self.sample_rate).max(0.0)
        } else {
            self.pos
        };
        let Some(l) = self.make_loop(start, start + length.0 * beat_len) else {
            return;
        };
        if self.roll.is_none() {
            self.roll = Some(RollSaved {
                stored: self.loop_stored,
                active: self.loop_active,
            });
        }
        self.begin_slip(SLIP_ROLL);
        self.enter_loop(l);
    }

    fn end_roll(&mut self) {
        let Some(saved) = self.roll.take() else {
            return;
        };
        self.loop_stored = saved.stored;
        self.loop_active = saved.active;
        let jumped = self.end_slip(SLIP_ROLL);
        if !jumped && self.key_lock && self.audible() && self.scratch.is_none() {
            self.move_to(self.pos);
        }
    }

    /// Multiplies the active loop length, keeping its start.
    pub(crate) fn loop_resize(&mut self, factor: f32, sink: &mut EventSink) {
        let Some(l) = self.loop_stored.filter(|_| self.loop_active) else {
            return;
        };
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let len = (l.len() * f64::from(factor)).max(self.min_loop());
        let Some(resized) = self.make_loop(l.start, l.start + len) else {
            return;
        };
        self.loop_stored = Some(resized);
        if self.pos >= resized.end {
            self.relocate(resized.start + (self.pos - resized.start).rem_euclid(resized.len()));
        }
        self.announce_loop(resized, sink);
    }

    /// Sets and activates an explicit loop (saved loop recall).
    pub(crate) fn set_loop(&mut self, region: FrameRegion) {
        let Some(end) = region.end else {
            return;
        };
        if self.track.is_none() {
            return;
        }
        let Some(l) = self.make_loop(region.start.0 as f64, end.0 as f64) else {
            return;
        };
        if self.slip && !self.loop_active {
            self.begin_slip(SLIP_LOOP);
        }
        self.loop_stored = Some(l);
        self.loop_active = true;
        self.loop_in = None;
        if self.pos >= l.end {
            self.relocate(l.start);
        }
    }
}
