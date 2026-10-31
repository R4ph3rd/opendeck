//! Jog wheel: scratch in vinyl mode, nudge otherwise, fast search.
//!
//! Platter motion arrives as small revolution deltas at the controller's
//! event rate, which has nothing to do with the audio block rate. Raw
//! per-block speeds would zipper, so motion is not applied directly. Each
//! turn adds to `jog_owed`, the source frames the record still has to travel,
//! and every block delivers a fixed fraction of it (time constant
//! [`SCRATCH_TAU`]). That is a first-order low pass on the speed: the same
//! feel at any block size, no distance lost, no overshoot. The render then
//! ramps the speed linearly from the previous block's to the new one.

use opendeck_contracts::Beats;
use opendeck_contracts::tempo::VINYL_SECONDS_PER_REVOLUTION;

use super::{Deck, SLIP_SCRATCH, Transport};

/// Time constant of the platter speed filter while the platter is held, seconds.
const SCRATCH_TAU: f64 = 0.020;

/// Time constant of the record spinning back up to tempo after release, seconds.
const RELEASE_TAU: f64 = 0.025;

/// Time constant of the nudge offset following the wheel, seconds.
const NUDGE_TAU: f64 = 0.040;

/// Nudge per revolution per second of wheel speed (`0.10` = +10 % at one turn
/// per second). TODO(verify): product tuning, open question in CONTEXT/engine.md.
const NUDGE_PER_REV_PER_SEC: f64 = 0.10;

/// Largest nudge, as a fraction of speed.
const MAX_NUDGE: f64 = 0.08;

/// The last few frames of motion are delivered in one block so a record that
/// the hand has stopped comes to rest instead of creeping.
const SETTLE_FRAMES: f64 = 8.0;

/// Largest scratch speed, as a ratio of original speed.
const SCRATCH_MAX_RATE: f64 = 6.0;

/// Furthest the record may be behind the platter, seconds of audio.
const MAX_OWED_SECONDS: f64 = 10.0;

/// Scratch state while the platter is held (or spinning back after release).
#[derive(Debug, Clone, Copy)]
pub(super) struct Scratch {
    /// Speed for the block being rendered, source frames per output frame.
    pub(super) rate: f64,
    /// Released: the record is returning to tempo.
    pub(super) releasing: bool,
}

impl Deck {
    /// Platter touch. In vinyl mode a touch holds the record and the wheel
    /// scratches; the track must be loaded.
    pub(crate) fn jog_touch(&mut self, touched: bool) {
        self.touched = touched;
        if self.track.is_none() {
            return;
        }
        if touched && self.vinyl {
            self.enter_scratch();
        } else if !touched {
            self.release_scratch();
        }
    }

    /// Wheel rotation since the last message, in revolutions.
    pub(crate) fn jog_turn(&mut self, revolutions: f32) {
        if !revolutions.is_finite() || self.track.is_none() {
            return;
        }
        let revs = f64::from(revolutions);
        match self.scratch {
            Some(s) if !s.releasing => {
                let limit = MAX_OWED_SECONDS * self.sample_rate;
                let frames = revs * VINYL_SECONDS_PER_REVOLUTION * self.sample_rate;
                self.jog_owed = (self.jog_owed + frames).clamp(-limit, limit);
            }
            _ => self.jog_revs += revs,
        }
    }

    /// Fast search by `beats` (SHIFT + jog). Without a beat grid a beat is
    /// taken as half a second (120 BPM).
    pub(crate) fn jog_search(&mut self, beats: Beats) {
        if self.track.is_none() || !beats.0.is_finite() {
            return;
        }
        let beat_len = self.beat_len().unwrap_or(0.5 * self.sample_rate);
        self.pending = None;
        self.move_to(self.pos + beats.0 * beat_len);
    }

    pub(crate) fn set_vinyl(&mut self, on: bool) {
        self.vinyl = on;
        if self.track.is_none() {
            return;
        }
        if on && self.touched {
            self.enter_scratch();
        } else if !on {
            self.release_scratch();
        }
    }

    fn enter_scratch(&mut self) {
        if let Some(s) = self.scratch.as_mut() {
            // Touched again while the record was spinning back up.
            s.releasing = false;
            return;
        }
        if self.slip && self.transport == Transport::Playing {
            self.begin_slip(SLIP_SCRATCH);
        }
        // Key lock cannot scratch: leave the stretcher for plain varispeed.
        let hand_over = self.stretching() && self.audible();
        if hand_over {
            self.start_tail();
        }
        self.pending = None;
        self.jog_owed = 0.0;
        self.nudge = 0.0;
        let rate = if self.audible() { self.rate_cur } else { 0.0 };
        self.scratch = Some(Scratch {
            rate,
            releasing: false,
        });
        if hand_over {
            self.begin_audible();
        }
    }

    fn release_scratch(&mut self) {
        let Some(s) = self.scratch.as_mut() else {
            return;
        };
        s.releasing = true;
        self.jog_owed = 0.0;
        // With slip the record snaps back to where playback would be.
        if self.slip_hold & SLIP_SCRATCH != 0 {
            self.slip_hold &= !SLIP_SCRATCH;
            if self.slip_hold == 0
                && self.transport == Transport::Playing
                && let Some(p) = self.slip_pos.take()
            {
                self.leave_scratch(Some(p));
            }
        }
    }

    /// Ends scratch mode. The scratching state fades out under the new one,
    /// which is plain playback at tempo (re-primed when key lock is on).
    fn leave_scratch(&mut self, jump_to: Option<f64>) {
        let pos = jump_to.unwrap_or(self.pos).clamp(0.0, self.len_frames());
        let audible_after = self.transport != Transport::Stopped;
        self.start_tail();
        self.scratch = None;
        self.pos = pos;
        if audible_after {
            self.begin_audible();
            self.mark_align();
        }
    }

    /// Per-block jog bookkeeping: the scratch speed for this block, or the
    /// nudge offset.
    pub(super) fn prepare_jog(&mut self, n: usize) {
        if n == 0 {
            return;
        }
        let dt = n as f64 / self.sample_rate;
        let natural = self.tempo_rate();
        let stopped = self.transport == Transport::Stopped;
        let mut leave = false;
        if let Some(s) = self.scratch.as_mut() {
            if s.releasing {
                let goal = if stopped { 0.0 } else { natural };
                s.rate += (goal - s.rate) * (1.0 - (-dt / RELEASE_TAU).exp());
                leave = (s.rate - goal).abs() < 0.002;
            } else {
                let mut take = self.jog_owed * (1.0 - (-dt / SCRATCH_TAU).exp());
                if self.jog_owed.abs() < SETTLE_FRAMES {
                    take = self.jog_owed;
                }
                // Speed is capped; what the cap holds back stays owed.
                s.rate = (take / n as f64).clamp(-SCRATCH_MAX_RATE, SCRATCH_MAX_RATE);
                self.jog_owed -= s.rate * n as f64;
            }
            self.nudge = 0.0;
            self.jog_revs = 0.0;
        } else {
            let wheel = self.jog_revs / dt;
            self.jog_revs = 0.0;
            let target = (wheel * NUDGE_PER_REV_PER_SEC).clamp(-MAX_NUDGE, MAX_NUDGE);
            self.nudge += (target - self.nudge) * (1.0 - (-dt / NUDGE_TAU).exp());
            if target == 0.0 && self.nudge.abs() < 1e-6 {
                self.nudge = 0.0;
            }
        }
        if leave {
            self.leave_scratch(None);
        }
    }
}
