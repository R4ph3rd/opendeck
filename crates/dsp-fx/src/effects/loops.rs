//! Buffer-capture effects: Helix, Roll, Reverse Roll, Vinyl Brake.

use crate::catalog::{EffectSpec, SyncSpec, param_index};
use crate::core::{Ctx, Dsp, Params, max_sync_samples};
use crate::dsp::Delay;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Recording into the ring, output silent (Helix with Hold off).
    Free,
    /// Recording one loop while passing the input through.
    Capture,
    /// Repeating the captured slice.
    Looping,
}

/// Per-render loop parameters in plain units.
#[derive(Clone, Copy)]
struct LoopParams {
    hold: bool,
    /// Per-repeat level loss, `[0, 1)`.
    decay: f32,
    fade_ms: f32,
    fallback_ms: f32,
}

/// Capture-and-repeat engine shared by Helix, Roll and Reverse Roll.
struct Looper {
    l: Vec<f32>,
    r: Vec<f32>,
    w: usize,
    filled: usize,
    state: State,
    len: usize,
    start: usize,
    count: usize,
    pos: usize,
    gain: f32,
    last_div: f64,
    /// Re-evaluate the loop length at the next render.
    pending: bool,
    reverse: bool,
    /// Helix: loop the already recorded history instead of recording anew.
    history: bool,
    hold: bool,
    sr: f32,
    spec: Option<&'static EffectSpec>,
}

impl Looper {
    fn new(reverse: bool, history: bool) -> Self {
        Self {
            l: vec![0.0; 16],
            r: vec![0.0; 16],
            w: 0,
            filled: 0,
            state: State::Free,
            len: 8,
            start: 0,
            count: 0,
            pos: 0,
            gain: 1.0,
            last_div: f64::NAN,
            pending: true,
            reverse,
            history,
            hold: false,
            sr: 48_000.0,
            spec: None,
        }
    }

    fn alloc(&mut self, sr: f32, spec: &'static EffectSpec) {
        let n = max_sync_samples(spec, sr);
        self.l.clear();
        self.l.resize(n, 0.0);
        self.r.clear();
        self.r.resize(n, 0.0);
        self.sr = sr;
        self.spec = Some(spec);
    }

    fn clear(&mut self) {
        self.l.fill(0.0);
        self.r.fill(0.0);
        self.w = 0;
        self.filled = 0;
        self.state = State::Free;
        self.count = 0;
        self.pos = 0;
        self.gain = 1.0;
        self.last_div = f64::NAN;
        self.pending = true;
    }

    /// Starts a loop of the current length, from history when allowed and available.
    fn arm(&mut self, ctx: &Ctx, fallback_ms: f32, sync: &SyncSpec) {
        let cap = self.l.len();
        self.len = (ctx.period_samples(sync, fallback_ms) as usize).clamp(8, cap - 1);
        self.pos = 0;
        self.count = 0;
        self.gain = 1.0;
        if self.history && self.filled >= self.len {
            self.start = (self.w + cap - self.len) % cap;
            self.state = State::Looping;
        } else {
            self.start = self.w;
            self.state = State::Capture;
        }
    }

    fn run(&mut self, l: &mut [f32], r: &mut [f32], ctx: &Ctx, lp: LoopParams) {
        let LoopParams {
            hold,
            decay,
            fade_ms,
            fallback_ms,
        } = lp;
        let Some(spec) = self.spec else { return };
        let Some(sync) = spec.sync else { return };
        if (ctx.division - self.last_div).abs() > 1e-9 {
            self.last_div = ctx.division;
            self.pending = true;
        }
        if self.history {
            if hold != self.hold {
                self.hold = hold;
                self.pending = true;
            }
            if !hold {
                self.state = State::Free;
                self.pending = false;
            }
        } else {
            self.hold = true;
        }
        if self.pending && self.hold {
            self.pending = false;
            self.arm(ctx, fallback_ms, &sync);
        }
        let cap = self.l.len();
        let fade = ((fade_ms * 0.001 * self.sr) as usize).clamp(1, (self.len / 2).max(1));
        let keep = 1.0 - decay;
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            match self.state {
                State::Free => {
                    self.l[self.w] = *xl;
                    self.r[self.w] = *xr;
                    self.w = (self.w + 1) % cap;
                    self.filled = (self.filled + 1).min(cap);
                    *xl = 0.0;
                    *xr = 0.0;
                }
                State::Capture => {
                    self.l[self.w] = *xl;
                    self.r[self.w] = *xr;
                    self.w = (self.w + 1) % cap;
                    self.filled = (self.filled + 1).min(cap);
                    self.count += 1;
                    if self.count >= self.len {
                        self.state = State::Looping;
                        self.pos = 0;
                        self.gain = 1.0;
                    }
                }
                State::Looping => {
                    let off = if self.reverse {
                        self.len - 1 - self.pos
                    } else {
                        self.pos
                    };
                    let idx = (self.start + off) % cap;
                    let env = if self.pos < fade {
                        self.pos as f32 / fade as f32
                    } else if self.pos + fade > self.len {
                        (self.len - self.pos) as f32 / fade as f32
                    } else {
                        1.0
                    };
                    let g = self.gain * env;
                    *xl = self.l[idx] * g;
                    *xr = self.r[idx] * g;
                    self.pos += 1;
                    if self.pos >= self.len {
                        self.pos = 0;
                        self.gain *= keep;
                    }
                }
            }
        }
    }
}

// --- Helix -----------------------------------------------------------------

const H_HOLD: usize = param_index("helix", "hold");
const H_TIME: usize = param_index("helix", "time");
const H_FADE: usize = param_index("helix", "fade");
const H_DECAY: usize = param_index("helix", "decay");

pub(crate) struct HelixFx(Looper);

impl Default for HelixFx {
    fn default() -> Self {
        Self(Looper::new(false, true))
    }
}

impl Dsp for HelixFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, spec: &'static EffectSpec) {
        self.0.alloc(sr, spec);
    }

    fn reset(&mut self) {
        self.0.clear();
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let lp = LoopParams {
            hold: p.on(H_HOLD),
            decay: p.v[H_DECAY] * 0.01,
            fade_ms: p.v[H_FADE],
            fallback_ms: p.v[H_TIME],
        };
        self.0.run(l, r, ctx, lp);
    }
}

// --- Roll and Reverse Roll -------------------------------------------------

const R_DECAY: usize = param_index("roll", "decay");
const R_FADE: usize = param_index("roll", "fade");
const R_TIME: usize = param_index("roll", "time");
const RR_DECAY: usize = param_index("reverse_roll", "decay");
const RR_FADE: usize = param_index("reverse_roll", "fade");
const RR_TIME: usize = param_index("reverse_roll", "time");

pub(crate) struct RollFx(Looper);

impl Default for RollFx {
    fn default() -> Self {
        Self(Looper::new(false, false))
    }
}

impl Dsp for RollFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, spec: &'static EffectSpec) {
        self.0.alloc(sr, spec);
    }

    fn reset(&mut self) {
        self.0.clear();
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let lp = LoopParams {
            hold: true,
            decay: p.v[R_DECAY] * 0.01,
            fade_ms: p.v[R_FADE],
            fallback_ms: p.v[R_TIME],
        };
        self.0.run(l, r, ctx, lp);
    }
}

pub(crate) struct ReverseRollFx(Looper);

impl Default for ReverseRollFx {
    fn default() -> Self {
        Self(Looper::new(true, false))
    }
}

impl Dsp for ReverseRollFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, spec: &'static EffectSpec) {
        self.0.alloc(sr, spec);
    }

    fn reset(&mut self) {
        self.0.clear();
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let lp = LoopParams {
            hold: true,
            decay: p.v[RR_DECAY] * 0.01,
            fade_ms: p.v[RR_FADE],
            fallback_ms: p.v[RR_TIME],
        };
        self.0.run(l, r, ctx, lp);
    }
}

// --- Vinyl Brake -----------------------------------------------------------

const V_CURVE: usize = param_index("vinyl_brake", "curve");
const V_TIME: usize = param_index("vinyl_brake", "time");

pub(crate) struct VinylBrakeFx {
    l: Delay,
    r: Delay,
    elapsed: f64,
    total: f64,
    lag: f32,
    last_div: f64,
    pending: bool,
    spec: Option<&'static EffectSpec>,
}

impl Default for VinylBrakeFx {
    fn default() -> Self {
        Self {
            l: Delay::default(),
            r: Delay::default(),
            elapsed: 0.0,
            total: 1.0,
            lag: 1.0,
            last_div: f64::NAN,
            pending: true,
            spec: None,
        }
    }
}

impl Dsp for VinylBrakeFx {
    fn prepare(&mut self, sr: f32, _max_block: usize, spec: &'static EffectSpec) {
        let n = max_sync_samples(spec, sr);
        self.l.alloc(n);
        self.r.alloc(n);
        self.spec = Some(spec);
    }

    fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.pending = true;
        self.last_div = f64::NAN;
    }

    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx) {
        let Some(sync) = self.spec.and_then(|s| s.sync) else {
            return;
        };
        if (ctx.division - self.last_div).abs() > 1e-9 {
            self.last_div = ctx.division;
            self.pending = true;
        }
        if self.pending {
            self.pending = false;
            self.total = ctx.period_samples(&sync, p.v[V_TIME]).max(64.0);
            self.elapsed = 0.0;
            self.lag = 1.0;
        }
        let k = 1.0 + 3.0 * p.v[V_CURVE] * 0.01;
        let max = self.l.max_delay();
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            let (inl, inr) = (*xl, *xr);
            if self.elapsed >= self.total {
                *xl = 0.0;
                *xr = 0.0;
            } else {
                let speed = (1.0 - self.elapsed / self.total).powf(f64::from(k)) as f32;
                let yl = self.l.read(self.lag);
                let yr = self.r.read(self.lag);
                let env = (speed * 8.0).min(1.0);
                *xl = yl * env;
                *xr = yr * env;
                self.lag = (self.lag + 1.0 - speed).min(max);
                self.elapsed += 1.0;
            }
            self.l.push(inl);
            self.r.push(inr);
        }
    }
}
