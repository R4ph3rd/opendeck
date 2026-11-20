//! The preview voice: auditions a track outside the decks.
//!
//! One voice, independent of decks 1 and 2 and of the sync leader. A decoded
//! buffer arrives as a [`TrackBuffer`] over the same hand-off ring as deck
//! loads and leaves on the same return ring, so the audio thread never
//! allocates or frees. Everything here is allocation free after
//! [`Preview::new`].
//!
//! # Routing (product choice)
//!
//! - Default: the preview joins the headphone bus after the cue/master blend
//!   and the split-cue switch, before the headphone level. So it is heard at
//!   unity whatever the blend (a blend of "master" would otherwise mute it),
//!   and the headphone level knob scales it. The master stays silent.
//! - No headphone pair on the device (`SetPreviewToMaster`, set by app-core
//!   from the negotiated routing): the preview joins the master after the
//!   master FX and the inputs, before the master level, at
//!   [`PREVIEW_MASTER_GAIN`] (-12 dB). It is not ducked by talk-over, and the
//!   master level and the booth copy follow it.
//!
//! # Behaviour
//!
//! - Loading a buffer starts playing at the given fraction. A new load, a
//!   `PreviewPlay` on a sounding voice and `PreviewStop` fade the old sound out
//!   over 2 ms; the new sound fades in over the same time. A replaced buffer
//!   stays alive until its fade ends, then goes to the return ring.
//! - Playback is at the engine rate, no tempo or key change. At the end of
//!   the buffer the voice stops by itself; `PreviewPlay` restarts it.

use opendeck_contracts::preview::PreviewTelemetry;
use opendeck_contracts::{EngineCommand, Frames, SampleRate, telemetry::EngineTelemetry};

use crate::track::TrackBuffer;

/// Level of the preview on the master when there is no headphone pair: -12 dB.
pub const PREVIEW_MASTER_GAIN: f32 = 0.251_188_64;

/// A position fading out after a jump or stop.
#[derive(Debug, Clone, Copy)]
struct Fading {
    pos: usize,
    left: usize,
}

/// A replaced buffer kept until its fade-out ends.
struct Outgoing {
    track: TrackBuffer,
    fade: Fading,
}

pub(crate) struct Preview {
    track: Option<TrackBuffer>,
    token: u32,
    /// Playhead in frames.
    pos: usize,
    playing: bool,
    /// Frames of fade-in still to apply.
    fade_in: usize,
    fading: Option<Fading>,
    outgoing: Option<Outgoing>,
    to_master: bool,
    declick: usize,
    bus: Vec<f32>,
    /// The bus holds sound this block.
    active: bool,
}

/// The stereo frame at `pos`, silence past the end.
#[inline]
fn frame(samples: &[f32], pos: usize) -> (f32, f32) {
    let at = pos.saturating_mul(2);
    match samples.get(at..at + 2) {
        Some([l, r]) => (*l, *r),
        _ => (0.0, 0.0),
    }
}

impl Preview {
    /// Allocates the bus. Not real-time safe.
    pub(crate) fn new(sample_rate: SampleRate, max_block_frames: usize) -> Self {
        let declick = ((sample_rate.as_f64() * 0.002) as usize).clamp(16, max_block_frames.max(16));
        Self {
            track: None,
            token: 0,
            pos: 0,
            playing: false,
            fade_in: 0,
            fading: None,
            outgoing: None,
            to_master: false,
            declick,
            bus: vec![0.0; max_block_frames.max(1) * 2],
            active: false,
        }
    }

    /// Puts `track` in the voice and starts it at `fraction`. The previous
    /// buffer fades out and comes back through [`take_finished`](Self::take_finished);
    /// when a second replacement arrives before the first fade ended, the
    /// older one is returned here (cut).
    pub(crate) fn load(
        &mut self,
        track: TrackBuffer,
        token: u32,
        fraction: f32,
    ) -> Option<TrackBuffer> {
        let mut returned = None;
        if let Some(old) = self.track.replace(track) {
            let fade = if self.playing {
                Fading {
                    pos: self.pos,
                    left: self.declick,
                }
            } else {
                Fading { pos: 0, left: 0 }
            };
            if let Some(prev) = self.outgoing.replace(Outgoing { track: old, fade }) {
                returned = Some(prev.track);
            }
        }
        // The fade of the previous position belongs to the previous buffer.
        self.fading = None;
        self.playing = false;
        self.token = token;
        self.start(fraction);
        returned
    }

    /// A replaced buffer whose fade-out is over, ready for the return ring.
    pub(crate) fn take_finished(&mut self) -> Option<TrackBuffer> {
        if self.outgoing.as_ref().is_some_and(|o| o.fade.left == 0) {
            return self.outgoing.take().map(|o| o.track);
        }
        None
    }

    /// Applies a preview command. Returns false for any other command.
    pub(crate) fn apply(&mut self, command: EngineCommand) -> bool {
        match command {
            EngineCommand::PreviewPlay { fraction } => self.start(fraction),
            EngineCommand::PreviewStop => self.stop(),
            EngineCommand::SetPreviewToMaster { on } => self.to_master = on,
            _ => return false,
        }
        true
    }

    fn start(&mut self, fraction: f32) {
        let Some(frames) = self.track.as_ref().map(TrackBuffer::frames) else {
            return;
        };
        let fraction = if fraction.is_nan() {
            0.0
        } else {
            f64::from(fraction.clamp(0.0, 1.0))
        };
        if self.playing {
            self.fading = Some(Fading {
                pos: self.pos,
                left: self.declick,
            });
        }
        // Stay inside the buffer so a click on the very end still plays a frame.
        let last = frames.saturating_sub(1);
        self.pos = ((fraction * frames as f64) as u64).min(last) as usize;
        self.playing = true;
        self.fade_in = self.declick;
    }

    fn stop(&mut self) {
        if self.playing {
            self.fading = Some(Fading {
                pos: self.pos,
                left: self.declick,
            });
            self.playing = false;
        }
    }

    /// True when the voice is sounding or fading, so the bus needs mixing.
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    /// True when the preview goes to the master instead of the headphones.
    pub(crate) fn to_master(&self) -> bool {
        self.to_master
    }

    /// Renders `frames` frames into the bus. Check [`is_active`](Self::is_active).
    pub(crate) fn render(&mut self, frames: usize) {
        let declick = self.declick as f32;
        let gain = if self.to_master {
            PREVIEW_MASTER_GAIN
        } else {
            1.0
        };
        let Some(bus) = self.bus.get_mut(..frames * 2) else {
            self.active = false;
            return;
        };
        self.active = self.playing || self.fading.is_some() || self.outgoing.is_some();
        if !self.active {
            return;
        }
        let track = self.track.as_ref().map(TrackBuffer::samples);
        let old = self
            .outgoing
            .as_ref()
            .map(|o| (o.track.samples(), o.fade.pos, o.fade.left));
        let (mut old_pos, mut old_left) = old.map_or((0, 0), |(_, p, l)| (p, l));
        let old_samples = old.map(|(s, _, _)| s);
        for out in bus.chunks_exact_mut(2) {
            let (mut l, mut r) = (0.0_f32, 0.0_f32);
            if let (true, Some(samples)) = (self.playing, track) {
                if self.pos >= samples.len() / 2 {
                    self.playing = false;
                } else {
                    let (a, b) = frame(samples, self.pos);
                    let g = if self.fade_in > 0 {
                        let g = 1.0 - self.fade_in as f32 / declick;
                        self.fade_in -= 1;
                        g
                    } else {
                        1.0
                    };
                    l += a * g;
                    r += b * g;
                    self.pos += 1;
                }
            }
            if let (Some(f), Some(samples)) = (self.fading.as_mut(), track) {
                let g = f.left as f32 / declick;
                let (a, b) = frame(samples, f.pos);
                l += a * g;
                r += b * g;
                f.pos += 1;
                f.left = f.left.saturating_sub(1);
            }
            if old_left > 0
                && let Some(samples) = old_samples
            {
                let g = old_left as f32 / declick;
                let (a, b) = frame(samples, old_pos);
                l += a * g;
                r += b * g;
                old_pos += 1;
                old_left = old_left.saturating_sub(1);
            }
            out[0] = l * gain;
            out[1] = r * gain;
            if self.fading.is_some_and(|f| f.left == 0) {
                self.fading = None;
            }
        }
        if let Some(o) = self.outgoing.as_mut() {
            o.fade = Fading {
                pos: old_pos,
                left: old_left,
            };
        }
        // Held until the fade ended and the buffer left on the return ring.
        self.active = self.playing || self.fading.is_some() || old_left > 0;
    }

    /// The rendered block, interleaved stereo.
    pub(crate) fn bus(&self, frames: usize) -> &[f32] {
        self.bus.get(..frames * 2).unwrap_or(&[])
    }

    /// Writes the voice state into a telemetry snapshot.
    pub(crate) fn fill_telemetry(&self, snapshot: &mut EngineTelemetry) {
        snapshot.preview = match &self.track {
            Some(t) => PreviewTelemetry {
                loaded: true,
                playing: self.playing,
                token: self.token,
                position: Frames(self.pos.min(t.frames() as usize) as u64),
                duration: t.duration(),
            },
            None => PreviewTelemetry::default(),
        };
    }
}
