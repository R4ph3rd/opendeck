//! Modulation and gating: Flanger, Phaser, Filter, Trans.

use crate::catalog::{EffectSpec, param_index};
use crate::core::{Ctx, Dsp, Params};
use crate::dsp::{Delay, Svf, SvfCoef, sine, smooth_coef, undenorm};
use std::f32::consts::PI;

/// An LFO cycle spans this many divisions.
const LFO_DIVISIONS: f64 = 4.0;

// --- Flanger ---------------------------------------------------------------

const F_RATE: usize = param_index("flanger", "rate");
const F_DEPTH: usize = param_index("flanger", "depth");
const F_FB: usize = param_index("flanger", "feedback");
const F_DELAY: usize = param_index("flanger", "delay");

#[derive(Default)]
pub(crate) struct FlangerFx {
    l: Delay,
    r: Delay,
    free: f64,
    sr: f32,
}

impl Dsp for FlangerFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        self.sr = sr;
        let n = (0.03 * sr) as usize;
        self.l.alloc(n);
        self.r.alloc(n);
    }

    fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.free = 0.0;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let (start, inc) = ctx.cycle(ctx.division * LFO_DIVISIONS, p.v[F_RATE], &mut self.free);
        let depth = p.v[F_DEPTH] * 0.01;
        let fb = p.v[F_FB] * 0.01;
        let base = p.v[F_DELAY] * 0.001 * self.sr;
        for (i, (xl, xr)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
            let ph = start + inc * i as f64;
            let dl = (base * (1.0 + depth * sine(ph))).max(2.0);
            let dr = (base * (1.0 + depth * sine(ph + 0.25))).max(2.0);
            let tl = self.l.read(dl);
            let tr = self.r.read(dr);
            self.l.push(*xl + fb * undenorm(tl));
            self.r.push(*xr + fb * undenorm(tr));
            *xl = 0.5 * (*xl + tl);
            *xr = 0.5 * (*xr + tr);
        }
    }
}

// --- Phaser ----------------------------------------------------------------

const P_RATE: usize = param_index("phaser", "rate");
const P_DEPTH: usize = param_index("phaser", "depth");
const P_FB: usize = param_index("phaser", "feedback");
const P_STAGES: usize = param_index("phaser", "stages");
const P_CENTER: usize = param_index("phaser", "center");

const MAX_STAGES: usize = 12;

#[derive(Default)]
pub(crate) struct PhaserFx {
    z: [[f32; MAX_STAGES]; 2],
    last: [f32; 2],
    free: f64,
    sr: f32,
}

impl Dsp for PhaserFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        self.sr = sr;
    }

    fn reset(&mut self) {
        self.z = [[0.0; MAX_STAGES]; 2];
        self.last = [0.0; 2];
        self.free = 0.0;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let (start, inc) = ctx.cycle(ctx.division * LFO_DIVISIONS, p.v[P_RATE], &mut self.free);
        let mid = start + inc * 0.5 * ctx.frames as f64;
        let depth = p.v[P_DEPTH] * 0.01;
        let fb = p.v[P_FB] * 0.01;
        let stages = (p.v[P_STAGES].round() as usize).clamp(2, MAX_STAGES);
        let mut a = [0.0_f32; 2];
        for (c, ac) in a.iter_mut().enumerate() {
            let lfo = sine(mid + 0.25 * c as f64);
            let f = (p.v[P_CENTER] * (depth * 1.5 * lfo).exp2()).clamp(20.0, 0.45 * self.sr);
            let t = (PI * f / self.sr).tan();
            *ac = (t - 1.0) / (t + 1.0);
        }
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            for (c, x) in [&mut *xl, &mut *xr].into_iter().enumerate() {
                let mut y = *x + fb * self.last[c];
                for z in &mut self.z[c][..stages] {
                    let out = a[c] * y + *z;
                    *z = undenorm(y - a[c] * out);
                    y = out;
                }
                self.last[c] = y;
                *x = 0.5 * (*x + y);
            }
        }
    }
}

// --- Filter ----------------------------------------------------------------

const FL_MODE: usize = param_index("filter", "mode");
const FL_LOW: usize = param_index("filter", "low");
const FL_HIGH: usize = param_index("filter", "high");
const FL_RES: usize = param_index("filter", "resonance");
const FL_DEPTH: usize = param_index("filter", "depth");
const FL_RATE: usize = param_index("filter", "rate");

#[derive(Default)]
pub(crate) struct FilterFx {
    svf: [Svf; 2],
    free: f64,
    sr: f32,
}

impl Dsp for FilterFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        self.sr = sr;
    }

    fn reset(&mut self) {
        self.svf = [Svf::default(); 2];
        self.free = 0.0;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let (start, inc) = ctx.cycle(ctx.division * LFO_DIVISIONS, p.v[FL_RATE], &mut self.free);
        let lfo = sine(start + inc * 0.5 * ctx.frames as f64);
        let (low, high) = (p.v[FL_LOW], p.v[FL_HIGH].max(p.v[FL_LOW] * 1.01));
        let x = 0.5 + 0.5 * p.v[FL_DEPTH] * 0.01 * lfo;
        let cutoff = low * (high / low).powf(x);
        let q = 0.7 + p.v[FL_RES] * 0.01 * 9.3;
        let c = SvfCoef::new(cutoff, q, self.sr);
        let hp = p.on(FL_MODE);
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            for (ch, x) in [&mut *xl, &mut *xr].into_iter().enumerate() {
                let o = self.svf[ch].run(&c, *x);
                *x = if hp { o.hp } else { o.lp };
            }
        }
    }
}

// --- Trans -----------------------------------------------------------------

const T_DEPTH: usize = param_index("trans", "depth");
const T_DUTY: usize = param_index("trans", "duty");
const T_SMOOTH: usize = param_index("trans", "smooth");
const T_RATE: usize = param_index("trans", "rate");

#[derive(Default)]
pub(crate) struct TransFx {
    gain: f32,
    free: f64,
}

impl Dsp for TransFx {
    fn prepare(&mut self, _sr: f32, _max_block: usize, _spec: &'static EffectSpec) {}

    fn reset(&mut self) {
        self.gain = 1.0;
        self.free = 0.0;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let (start, inc) = ctx.cycle(ctx.division, p.v[T_RATE], &mut self.free);
        let duty = f64::from(p.v[T_DUTY] * 0.01);
        let closed = 1.0 - p.v[T_DEPTH] * 0.01;
        let a = smooth_coef(1, ctx.sr as f32, p.v[T_SMOOTH] * 0.001);
        for (i, (xl, xr)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
            let ph = (start + inc * i as f64).rem_euclid(1.0);
            let target = if ph < duty { 1.0 } else { closed };
            self.gain += a * (target - self.gain);
            *xl *= self.gain;
            *xr *= self.gain;
        }
    }
}
