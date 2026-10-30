//! Reverb: eight damped parallel combs and four series all-passes per
//! channel (Schroeder/Moorer topology, public-domain delay-length ratios).

use crate::catalog::{EffectSpec, param_index};
use crate::core::{Ctx, Dsp, Params};
use crate::dsp::{Delay, undenorm};

const SIZE: usize = param_index("reverb", "size");
const DAMP: usize = param_index("reverb", "damping");
const PRE: usize = param_index("reverb", "predelay");
const WIDTH: usize = param_index("reverb", "width");

/// Delay lengths at 44.1 kHz.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS: [usize; 4] = [556, 441, 341, 225];
const SPREAD: usize = 23;
const INPUT_GAIN: f32 = 0.15;

#[derive(Default, Clone)]
struct Comb {
    line: Delay,
    len: f32,
    store: f32,
}

impl Comb {
    fn alloc(&mut self, len: usize) {
        self.line.alloc(len + 2);
        self.len = len as f32;
        self.store = 0.0;
    }

    #[inline]
    fn run(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.line.read(self.len);
        self.store = undenorm(y * (1.0 - damp) + self.store * damp);
        self.line.push(x + self.store * feedback);
        y
    }
}

#[derive(Default, Clone)]
struct Allpass {
    line: Delay,
    len: f32,
}

impl Allpass {
    fn alloc(&mut self, len: usize) {
        self.line.alloc(len + 2);
        self.len = len as f32;
    }

    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let buffered = self.line.read(self.len);
        self.line.push(undenorm(x + buffered * 0.5));
        buffered - x
    }
}

#[derive(Default)]
struct Channel {
    combs: [Comb; 8],
    aps: [Allpass; 4],
}

#[derive(Default)]
pub(crate) struct ReverbFx {
    ch: [Channel; 2],
    pre: Delay,
    pre_len: f32,
}

impl Dsp for ReverbFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, _spec: &'static EffectSpec) {
        let k = f64::from(sr) / 44_100.0;
        let scale = |n: usize| ((n as f64 * k).round() as usize).max(2);
        for (c, ch) in self.ch.iter_mut().enumerate() {
            let spread = c * SPREAD;
            for (comb, n) in ch.combs.iter_mut().zip(COMBS) {
                comb.alloc(scale(n + spread));
            }
            for (ap, n) in ch.aps.iter_mut().zip(ALLPASS) {
                ap.alloc(scale(n + spread));
            }
        }
        self.pre.alloc((0.1 * sr) as usize + 4);
    }

    fn reset(&mut self) {
        for ch in &mut self.ch {
            for c in &mut ch.combs {
                c.line.clear();
                c.store = 0.0;
            }
            for a in &mut ch.aps {
                a.line.clear();
            }
        }
        self.pre.clear();
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let feedback = 0.7 + 0.28 * p.v[SIZE] * 0.01;
        let damp = p.v[DAMP] * 0.01 * 0.4;
        let width = p.v[WIDTH] * 0.01;
        let w1 = 0.5 * (1.0 + width);
        let w2 = 0.5 * (1.0 - width);
        self.pre_len = (p.v[PRE] * 0.001 * ctx.sr as f32).max(1.0);
        let use_pre = p.v[PRE] > 0.05;
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let mono = 0.5 * (*xl + *xr);
            let x = if use_pre {
                let y = self.pre.read(self.pre_len);
                self.pre.push(mono);
                y
            } else {
                self.pre.push(mono);
                mono
            } * INPUT_GAIN;
            let mut out = [0.0_f32; 2];
            for (o, ch) in out.iter_mut().zip(self.ch.iter_mut()) {
                let mut sum = 0.0;
                for c in &mut ch.combs {
                    sum += c.run(x, feedback, damp);
                }
                for a in &mut ch.aps {
                    sum = a.run(sum);
                }
                *o = sum;
            }
            *xl = out[0] * w1 + out[1] * w2;
            *xr = out[1] * w1 + out[0] * w2;
        }
    }
}
