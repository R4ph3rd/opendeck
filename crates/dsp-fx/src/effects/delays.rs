//! Delay family: Delay, Echo, Ping Pong, Spiral.

use crate::catalog::{EffectSpec, SyncSpec, param_index};
use crate::core::{Ctx, Dsp, Params, max_sync_samples};
use crate::dsp::{Delay, StereoPitch, lp_coef, smooth_coef, undenorm};

/// Slews the delay time so tempo or division changes glide instead of click.
/// The first target after a reset is taken exactly.
#[derive(Debug, Clone, Copy)]
struct TimeSlew {
    cur: f32,
    fresh: bool,
    coef: f32,
}

impl Default for TimeSlew {
    fn default() -> Self {
        Self {
            cur: 2.0,
            fresh: true,
            coef: 0.001,
        }
    }
}

impl TimeSlew {
    fn init(&mut self, sr: f32) {
        self.coef = smooth_coef(1, sr, 0.05);
        self.fresh = true;
    }

    fn set(&mut self, target: f32) -> f32 {
        if self.fresh {
            self.cur = target;
            self.fresh = false;
        }
        target
    }

    #[inline]
    fn next(&mut self, target: f32) -> f32 {
        self.cur += self.coef * (target - self.cur);
        self.cur
    }
}

fn sync_of(spec: &EffectSpec) -> SyncSpec {
    spec.sync.unwrap_or(SyncSpec {
        meaning: "",
        min_beats: 0.03125,
        max_beats: 4.0,
        default_beats: 1.0,
    })
}

/// Target delay in samples for this piece, clamped to the line.
fn target_delay(ctx: &Ctx, sync: &SyncSpec, fallback_ms: f32, line: &Delay, minus: f32) -> f32 {
    let d = ctx.period_samples(sync, fallback_ms) as f32 - minus;
    d.clamp(2.0, line.max_delay())
}

macro_rules! stereo_delay_common {
    () => {
        fn prepare(&mut self, sr: f32, _max_block: usize, spec: &'static EffectSpec) {
            self.sr = sr;
            self.spec = Some(spec);
            let n = max_sync_samples(spec, sr);
            self.l.alloc(n);
            self.r.alloc(n);
            self.slew.init(sr);
            self.alloc_extra(sr);
        }
    };
}

// --- Delay -----------------------------------------------------------------

const D_TIME: usize = param_index("delay", "time");
const D_FB: usize = param_index("delay", "feedback");
const D_TONE: usize = param_index("delay", "tone");

#[derive(Default)]
pub(crate) struct DelayFx {
    l: Delay,
    r: Delay,
    slew: TimeSlew,
    lpl: f32,
    lpr: f32,
    sr: f32,
    spec: Option<&'static EffectSpec>,
}

impl DelayFx {
    fn alloc_extra(&mut self, _sr: f32) {}
}

impl Dsp for DelayFx {
    stereo_delay_common!();

    fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.lpl = 0.0;
        self.lpr = 0.0;
        self.slew.fresh = true;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let Some(spec) = self.spec else { return };
        let target = self
            .slew
            .set(target_delay(ctx, &sync_of(spec), p.v[D_TIME], &self.l, 0.0));
        let fb = p.v[D_FB] * 0.01;
        let a = lp_coef(p.v[D_TONE], self.sr);
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let d = self.slew.next(target);
            let yl = self.l.read(d);
            let yr = self.r.read(d);
            self.lpl += a * (yl - self.lpl);
            self.lpr += a * (yr - self.lpr);
            self.l.push(*xl + fb * undenorm(self.lpl));
            self.r.push(*xr + fb * undenorm(self.lpr));
            *xl = yl;
            *xr = yr;
        }
    }
}

// --- Echo ------------------------------------------------------------------

const E_TIME: usize = param_index("echo", "time");
const E_DECAY: usize = param_index("echo", "decay");
const E_DAMP: usize = param_index("echo", "damping");

#[derive(Default)]
pub(crate) struct EchoFx {
    l: Delay,
    r: Delay,
    slew: TimeSlew,
    lpl: f32,
    lpr: f32,
    hpl: f32,
    hpr: f32,
    sr: f32,
    spec: Option<&'static EffectSpec>,
}

impl EchoFx {
    fn alloc_extra(&mut self, _sr: f32) {}
}

impl Dsp for EchoFx {
    stereo_delay_common!();

    fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.lpl = 0.0;
        self.lpr = 0.0;
        self.hpl = 0.0;
        self.hpr = 0.0;
        self.slew.fresh = true;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let Some(spec) = self.spec else { return };
        let target = self
            .slew
            .set(target_delay(ctx, &sync_of(spec), p.v[E_TIME], &self.l, 0.0));
        let fb = p.v[E_DECAY] * 0.01;
        let a = lp_coef(p.v[E_DAMP], self.sr);
        let ah = lp_coef(100.0, self.sr);
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let d = self.slew.next(target);
            let yl = self.l.read(d);
            let yr = self.r.read(d);
            self.lpl += a * (yl - self.lpl);
            self.lpr += a * (yr - self.lpr);
            self.hpl += ah * (self.lpl - self.hpl);
            self.hpr += ah * (self.lpr - self.hpr);
            let fl = (self.lpl - self.hpl).tanh();
            let fr = (self.lpr - self.hpr).tanh();
            self.l.push(*xl + fb * undenorm(fl));
            self.r.push(*xr + fb * undenorm(fr));
            *xl = yl;
            *xr = yr;
        }
    }
}

// --- Ping Pong -------------------------------------------------------------

const P_TIME: usize = param_index("ping_pong", "time");
const P_FB: usize = param_index("ping_pong", "feedback");
const P_WIDTH: usize = param_index("ping_pong", "width");

#[derive(Default)]
pub(crate) struct PingPongFx {
    l: Delay,
    r: Delay,
    slew: TimeSlew,
    sr: f32,
    spec: Option<&'static EffectSpec>,
}

impl PingPongFx {
    fn alloc_extra(&mut self, _sr: f32) {}
}

impl Dsp for PingPongFx {
    stereo_delay_common!();

    fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.slew.fresh = true;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let Some(spec) = self.spec else { return };
        let target = self
            .slew
            .set(target_delay(ctx, &sync_of(spec), p.v[P_TIME], &self.l, 0.0));
        let fb = p.v[P_FB] * 0.01;
        let width = p.v[P_WIDTH] * 0.01;
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let d = self.slew.next(target);
            let mono = 0.5 * (*xl + *xr);
            let tl = self.l.read(d);
            let tr = self.r.read(d);
            self.l.push(mono + fb * undenorm(tr));
            self.r.push(fb * tl);
            // Width blends the centred sum of both taps with the bounce.
            let mid = 0.5 * (tl + tr);
            *xl = mid + width * (tl - mid);
            *xr = mid + width * (tr - mid);
        }
    }
}

// --- Spiral ----------------------------------------------------------------

const S_TIME: usize = param_index("spiral", "time");
const S_FB: usize = param_index("spiral", "feedback");
const S_PITCH: usize = param_index("spiral", "pitch");

const SPIRAL_GRAIN_S: f32 = 0.03;

#[derive(Default)]
pub(crate) struct SpiralFx {
    l: Delay,
    r: Delay,
    ps: StereoPitch,
    slew: TimeSlew,
    lpl: f32,
    lpr: f32,
    sr: f32,
    spec: Option<&'static EffectSpec>,
}

impl SpiralFx {
    fn alloc_extra(&mut self, sr: f32) {
        self.ps.alloc(sr, SPIRAL_GRAIN_S);
    }
}

impl Dsp for SpiralFx {
    stereo_delay_common!();

    fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.ps.clear();
        self.lpl = 0.0;
        self.lpr = 0.0;
        self.slew.fresh = true;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let Some(spec) = self.spec else { return };
        // The shifter adds about half a grain; take it out of the loop time.
        let comp = 0.5 * self.ps.grain();
        let target = self.slew.set(target_delay(
            ctx,
            &sync_of(spec),
            p.v[S_TIME],
            &self.l,
            comp,
        ));
        let fb = p.v[S_FB] * 0.01;
        let ratio = (p.v[S_PITCH] / 12.0).exp2();
        let a = lp_coef(6000.0, self.sr);
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let d = self.slew.next(target);
            let yl = self.l.read(d);
            let yr = self.r.read(d);
            self.lpl += a * (yl - self.lpl);
            self.lpr += a * (yr - self.lpr);
            let (sl, sr) = self.ps.process(self.lpl, self.lpr, ratio);
            self.l.push(*xl + fb * undenorm(sl));
            self.r.push(*xr + fb * undenorm(sr));
            *xl = yl;
            *xr = yr;
        }
    }
}
