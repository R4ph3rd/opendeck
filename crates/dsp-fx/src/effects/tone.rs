//! Pitch, Noise, Bit Crusher and the Colour Filter.

use crate::catalog::{EffectSpec, param_index};
use crate::core::{Ctx, Dsp, Params};
use crate::dsp::{Rng, StereoPitch, Svf, SvfCoef, sine};

// --- Pitch -----------------------------------------------------------------

const PI_SHIFT: usize = param_index("pitch", "shift");
const PITCH_GRAIN_S: f32 = 0.04;

#[derive(Default)]
pub(crate) struct PitchFx {
    ps: StereoPitch,
    latency: u32,
}

impl Dsp for PitchFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        self.ps.alloc(sr, PITCH_GRAIN_S);
        self.latency = (0.5 * self.ps.grain()) as u32;
    }

    fn reset(&mut self) {
        self.ps.clear();
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, _ctx: &Ctx) {
        let ratio = (p.v[PI_SHIFT] / 12.0).exp2();
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let (yl, yr) = self.ps.process(*xl, *xr, ratio);
            *xl = yl;
            *xr = yr;
        }
    }

    fn latency(&self) -> u32 {
        self.latency
    }
}

// --- Noise -----------------------------------------------------------------

const N_LEVEL: usize = param_index("noise", "level");
const N_CUTOFF: usize = param_index("noise", "cutoff");
const N_RES: usize = param_index("noise", "resonance");
const N_SWEEP: usize = param_index("noise", "sweep");
const N_RATE: usize = param_index("noise", "rate");
const N_MODE: usize = param_index("noise", "mode");

pub(crate) struct NoiseFx {
    svf: [Svf; 2],
    rng: [Rng; 2],
    free: f64,
    sr: f32,
}

impl Default for NoiseFx {
    fn default() -> Self {
        Self {
            svf: [Svf::default(); 2],
            rng: [Rng::new(0x1234_5678), Rng::new(0x9e37_79b9)],
            free: 0.0,
            sr: 48_000.0,
        }
    }
}

impl Dsp for NoiseFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        self.sr = sr;
    }

    fn reset(&mut self) {
        self.svf = [Svf::default(); 2];
        self.free = 0.0;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        // Free-running sweep: the Rate parameter is the only clock.
        let inc = f64::from(p.v[N_RATE]) / ctx.sr;
        let mid = self.free + inc * 0.5 * ctx.frames as f64;
        self.free = (self.free + inc * ctx.frames as f64).rem_euclid(1.0);
        let cutoff = p.v[N_CUTOFF] * (p.v[N_SWEEP] * 0.01 * 2.0 * sine(mid)).exp2();
        let q = 0.7 + p.v[N_RES] * 0.01 * 9.3;
        let c = SvfCoef::new(cutoff, q, self.sr);
        let level = p.v[N_LEVEL] * 0.01 * 0.5;
        let mode = p.v[N_MODE].round() as i32;
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            for (ch, x) in [&mut *xl, &mut *xr].into_iter().enumerate() {
                let o = self.svf[ch].run(&c, self.rng[ch].next());
                let n = match mode {
                    0 => o.lp,
                    1 => o.bp,
                    _ => o.hp,
                };
                *x += level * n;
            }
        }
    }
}

// --- Bit Crusher -----------------------------------------------------------

const B_BITS: usize = param_index("bit_crusher", "bits");
const B_RATE: usize = param_index("bit_crusher", "rate");

#[derive(Default)]
pub(crate) struct BitCrusherFx {
    acc: f32,
    held: [f32; 2],
}

impl Dsp for BitCrusherFx {
    fn prepare(&mut self, _sr: f32, _max_block: usize, _spec: &'static EffectSpec) {}

    fn reset(&mut self) {
        self.acc = 1.0;
        self.held = [0.0; 2];
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let levels = (p.v[B_BITS] - 1.0).exp2();
        let step = (p.v[B_RATE] / ctx.sr as f32).min(1.0);
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            self.acc += step;
            if self.acc >= 1.0 {
                self.acc -= 1.0;
                self.held = [
                    (*xl * levels).round() / levels,
                    (*xr * levels).round() / levels,
                ];
            }
            *xl = self.held[0];
            *xr = self.held[1];
        }
    }
}

// --- Colour Filter ---------------------------------------------------------

const C_COLOUR: usize = param_index("colour_filter", "colour");
const C_RES: usize = param_index("colour_filter", "resonance");

/// Butterworth Q values of a fourth-order filter built from two stages.
const Q_STAGE1: f32 = 0.541_196;
const Q_STAGE2: f32 = 1.306_563;
/// Colour magnitudes below this crossfade to the unfiltered signal.
const FLAT_ZONE: f32 = 0.05;

#[derive(Default)]
pub(crate) struct ColourFilterFx {
    svf: [[Svf; 2]; 2],
    sr: f32,
}

impl Dsp for ColourFilterFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        self.sr = sr;
    }

    fn reset(&mut self) {
        self.svf = [[Svf::default(); 2]; 2];
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, _ctx: &Ctx) {
        let colour = p.v[C_COLOUR] * 0.01;
        let amount = colour.abs();
        let high_pass = colour > 0.0;
        let cutoff = if high_pass {
            20.0 * (8000.0_f32 / 20.0).powf(amount)
        } else {
            20_000.0 * (100.0_f32 / 20_000.0).powf(amount)
        };
        let q2 = Q_STAGE2 * (1.0 + p.v[C_RES] * 0.01 * 3.0);
        let c1 = SvfCoef::new(cutoff, Q_STAGE1, self.sr);
        let c2 = SvfCoef::new(cutoff, q2, self.sr);
        let w = (amount / FLAT_ZONE).min(1.0);
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            for (ch, x) in [&mut *xl, &mut *xr].into_iter().enumerate() {
                let a = self.svf[0][ch].run(&c1, *x);
                let a = if high_pass { a.hp } else { a.lp };
                let b = self.svf[1][ch].run(&c2, a);
                let y = if high_pass { b.hp } else { b.lp };
                *x += w * (y - *x);
            }
        }
    }
}
