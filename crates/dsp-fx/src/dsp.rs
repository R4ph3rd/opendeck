//! Small DSP building blocks shared by the effects. Everything here is
//! allocation-free after construction except the explicit `alloc` methods,
//! which `prepare` calls.

use std::f32::consts::{PI, TAU};

/// Added to feedback paths so they never settle into denormal numbers.
const DENORM: f32 = 1e-20;

#[inline]
pub(crate) fn undenorm(x: f32) -> f32 {
    x + DENORM
}

/// One-pole smoothing coefficient for `frames` samples with time constant `tau_s`.
pub(crate) fn smooth_coef(frames: usize, sr: f32, tau_s: f32) -> f32 {
    1.0 - (-(frames as f32) / (sr * tau_s)).exp()
}

/// One-pole low-pass coefficient for a cutoff in Hz.
pub(crate) fn lp_coef(freq: f32, sr: f32) -> f32 {
    1.0 - (-TAU * freq.clamp(1.0, 0.49 * sr) / sr).exp()
}

/// Circular delay line with linear interpolation. `read(d)` returns the
/// sample written `d` pushes ago (`d >= 1`).
#[derive(Debug, Clone)]
pub(crate) struct Delay {
    buf: Vec<f32>,
    mask: usize,
    w: usize,
}

impl Default for Delay {
    fn default() -> Self {
        Self {
            buf: vec![0.0; 4],
            mask: 3,
            w: 0,
        }
    }
}

impl Delay {
    /// Sizes for delays up to `max_delay` samples and clears. Allocates.
    pub fn alloc(&mut self, max_delay: usize) {
        let n = (max_delay + 4).next_power_of_two();
        self.buf.clear();
        self.buf.resize(n, 0.0);
        self.mask = n - 1;
        self.w = 0;
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.w = 0;
    }

    /// Longest readable delay in samples.
    pub fn max_delay(&self) -> f32 {
        (self.buf.len() - 3) as f32
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.w] = x;
        self.w = (self.w + 1) & self.mask;
    }

    #[inline]
    pub fn read(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, self.max_delay());
        let di = d as usize;
        let f = d - di as f32;
        let i0 = self.w.wrapping_sub(di) & self.mask;
        let i1 = i0.wrapping_sub(1) & self.mask;
        self.buf[i0] * (1.0 - f) + self.buf[i1] * f
    }
}

/// Coefficients of the Cytomic/Zavalishin trapezoidal state-variable filter.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SvfCoef {
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

impl SvfCoef {
    pub fn new(freq: f32, q: f32, sr: f32) -> Self {
        let f = freq.clamp(10.0, 0.45 * sr);
        let g = (PI * f / sr).tan();
        let k = 1.0 / q.max(0.05);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            a1,
            a2,
            a3: g * a2,
            k,
        }
    }
}

/// Filter state for one channel.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Svf {
    ic1: f32,
    ic2: f32,
}

/// Outputs of one SVF step.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SvfOut {
    pub lp: f32,
    /// Band-pass with unity peak gain.
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    #[inline]
    pub fn run(&mut self, c: &SvfCoef, x: f32) -> SvfOut {
        let v3 = x - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = undenorm(2.0 * v1 - self.ic1);
        self.ic2 = undenorm(2.0 * v2 - self.ic2);
        SvfOut {
            lp: v2,
            bp: c.k * v1,
            hp: x - c.k * v1 - v2,
        }
    }
}

/// Xorshift noise source, `[-1, 1)`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rng(u32);

impl Rng {
    pub const fn new(seed: u32) -> Self {
        Self(seed)
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32) * (2.0 / 4_294_967_296.0) - 1.0
    }
}

/// Sine LFO value for a phase in cycles (any real number).
#[inline]
pub(crate) fn sine(phase: f64) -> f32 {
    ((phase.rem_euclid(1.0)) as f32 * TAU).sin()
}

/// Granular two-tap pitch shifter for a stereo pair (shared grain phase).
#[derive(Debug, Clone, Default)]
pub(crate) struct StereoPitch {
    l: Delay,
    r: Delay,
    grain: f32,
    d: f32,
}

impl StereoPitch {
    /// Allocates for a grain of `grain_s` seconds.
    pub fn alloc(&mut self, sr: f32, grain_s: f32) {
        self.grain = (grain_s * sr).max(8.0);
        let n = self.grain as usize + 4;
        self.l.alloc(n);
        self.r.alloc(n);
        self.d = self.grain * 0.25;
    }

    pub fn clear(&mut self) {
        self.l.clear();
        self.r.clear();
        self.d = self.grain * 0.25;
    }

    /// Grain length in samples.
    pub fn grain(&self) -> f32 {
        self.grain
    }

    #[inline]
    pub fn process(&mut self, xl: f32, xr: f32, ratio: f32) -> (f32, f32) {
        self.l.push(xl);
        self.r.push(xr);
        let g = self.grain;
        let d1 = self.d;
        let d2 = (d1 + 0.5 * g) % g;
        let s = (PI * d1 / g).sin();
        let w1 = s * s;
        let w2 = 1.0 - w1;
        let yl = w1 * self.l.read(d1 + 1.0) + w2 * self.l.read(d2 + 1.0);
        let yr = w1 * self.r.read(d1 + 1.0) + w2 * self.r.read(d2 + 1.0);
        self.d = (self.d + 1.0 - ratio).rem_euclid(g);
        (yl, yr)
    }
}
