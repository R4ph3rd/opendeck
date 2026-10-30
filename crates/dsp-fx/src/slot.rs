//! Reference slot host: wraps one effect with the host duties of ADR 0006
//! (equal-power wet/dry, click-free bypass crossfade, tail spill after
//! bypass, dry-path latency compensation).
//!
//! The engine owns the real slot host. This one exists so effects can be
//! used and tested standalone with the same semantics, and as an executable
//! description of what the effects expect from their host: call `reset()` on
//! engage (re-arms Roll, Reverse Roll, Helix and Vinyl Brake) and feed
//! silence, not nothing, while a tail decays.

use opendeck_contracts::fx::{Effect, ProcessContext, StereoBufferMut};
use opendeck_contracts::units::SampleRate;
use std::f32::consts::FRAC_PI_2;

/// Tail is considered finished below -90 dBFS.
const TAIL_FLOOR: f32 = 3.162_277_7e-5;
/// Consecutive quiet samples before a tail is declared finished.
const TAIL_QUIET_SAMPLES: usize = 4096;
/// Bypass crossfade and mix smoothing time.
const RAMP_S: f32 = 0.01;

/// One effect plus its host state.
pub struct FxSlot {
    fx: Box<dyn Effect>,
    has_tail: bool,
    enabled: bool,
    active: bool,
    engage: f32,
    mix_target: f32,
    mix: f32,
    ramp_step: f32,
    mix_coef: f32,
    quiet: usize,
    max_block: usize,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    dry_l: Vec<f32>,
    dry_r: Vec<f32>,
    ring_l: Vec<f32>,
    ring_r: Vec<f32>,
    ring_pos: usize,
    latency: usize,
}

impl FxSlot {
    /// Wraps `fx`, disabled, mix 1.0. Call [`FxSlot::prepare`] before processing.
    pub fn new(fx: Box<dyn Effect>) -> Self {
        let has_tail = fx.info().has_tail;
        Self {
            fx,
            has_tail,
            enabled: false,
            active: false,
            engage: 0.0,
            mix_target: 1.0,
            mix: 1.0,
            ramp_step: 0.01,
            mix_coef: 0.01,
            quiet: 0,
            max_block: 0,
            in_l: Vec::new(),
            in_r: Vec::new(),
            dry_l: Vec::new(),
            dry_r: Vec::new(),
            ring_l: Vec::new(),
            ring_r: Vec::new(),
            ring_pos: 0,
            latency: 0,
        }
    }

    /// Prepares the effect and allocates the slot buffers. Control thread.
    pub fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize) {
        self.fx.prepare(sample_rate, max_block_frames);
        let sr = sample_rate.0 as f32;
        self.ramp_step = 1.0 / (RAMP_S * sr).max(1.0);
        self.mix_coef = 1.0 - (-1.0 / (RAMP_S * sr)).exp();
        self.max_block = max_block_frames.max(1);
        for b in [
            &mut self.in_l,
            &mut self.in_r,
            &mut self.dry_l,
            &mut self.dry_r,
        ] {
            b.clear();
            b.resize(self.max_block, 0.0);
        }
        self.latency = self.fx.latency_frames() as usize;
        for b in [&mut self.ring_l, &mut self.ring_r] {
            b.clear();
            b.resize(self.latency, 0.0);
        }
        self.ring_pos = 0;
    }

    /// The wrapped effect, for parameter changes.
    pub fn effect_mut(&mut self) -> &mut dyn Effect {
        self.fx.as_mut()
    }

    /// Wet/dry mix `[0, 1]`, smoothed.
    pub fn set_mix(&mut self, mix: f32) {
        if mix.is_finite() {
            self.mix_target = mix.clamp(0.0, 1.0);
        }
    }

    /// Enables or bypasses. Enabling an idle slot resets the effect.
    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && !self.enabled && !self.active {
            self.fx.reset();
        }
        self.enabled = enabled;
        if enabled {
            self.active = true;
            self.quiet = 0;
        }
    }

    /// True while the effect is being processed (enabled, fading or spilling a tail).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Processes one block in place.
    pub fn process(&mut self, io: StereoBufferMut<'_>, ctx: &ProcessContext) {
        let mut io = io;
        let (l, r) = io.channels();
        let total = l.len();
        let mut pos = 0;
        while pos < total {
            let n = (total - pos).min(self.max_block);
            self.process_piece(&mut l[pos..pos + n], &mut r[pos..pos + n], ctx);
            pos += n;
        }
    }

    fn process_piece(&mut self, l: &mut [f32], r: &mut [f32], ctx: &ProcessContext) {
        let n = l.len();
        // Dry path, delayed by the effect latency so wet and dry line up.
        if self.ring_l.is_empty() {
            self.dry_l[..n].copy_from_slice(l);
            self.dry_r[..n].copy_from_slice(r);
        } else {
            for i in 0..n {
                self.dry_l[i] = self.ring_l[self.ring_pos];
                self.dry_r[i] = self.ring_r[self.ring_pos];
                self.ring_l[self.ring_pos] = l[i];
                self.ring_r[self.ring_pos] = r[i];
                self.ring_pos = (self.ring_pos + 1) % self.ring_l.len();
            }
        }
        if !self.active {
            l.copy_from_slice(&self.dry_l[..n]);
            r.copy_from_slice(&self.dry_r[..n]);
            return;
        }
        // Effect input fades with the engage ramp; the tail then runs on silence.
        let target = if self.enabled { 1.0 } else { 0.0 };
        let mut engage = self.engage;
        let mut mix = self.mix;
        for i in 0..n {
            engage += (target - engage).clamp(-self.ramp_step, self.ramp_step);
            self.in_l[i] = l[i] * engage;
            self.in_r[i] = r[i] * engage;
        }
        if let Some(io) = StereoBufferMut::new(&mut self.in_l[..n], &mut self.in_r[..n]) {
            self.fx.process(io, ctx);
        }
        engage = self.engage;
        let mut peak = 0.0_f32;
        for i in 0..n {
            engage += (target - engage).clamp(-self.ramp_step, self.ramp_step);
            mix += self.mix_coef * (self.mix_target - mix);
            let (wg, dg) = {
                let a = mix * FRAC_PI_2;
                (a.sin(), a.cos())
            };
            let wet_gain = if self.has_tail { wg } else { wg * engage };
            let dry_gain = (1.0 - engage) + engage * dg;
            l[i] = self.dry_l[i] * dry_gain + self.in_l[i] * wet_gain;
            r[i] = self.dry_r[i] * dry_gain + self.in_r[i] * wet_gain;
            peak = peak.max(self.in_l[i].abs()).max(self.in_r[i].abs());
        }
        self.engage = engage;
        self.mix = mix;
        if !self.enabled && self.engage <= 0.0 {
            if !self.has_tail {
                self.active = false;
            } else if peak < TAIL_FLOOR {
                self.quiet += n;
                if self.quiet >= TAIL_QUIET_SAMPLES {
                    self.active = false;
                }
            } else {
                self.quiet = 0;
            }
        }
    }
}
