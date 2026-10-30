//! Shared effect plumbing: parameter bank with smoothing, per-piece context,
//! the [`Dsp`] trait the effects implement and the [`Fx`] adapter that turns
//! a `Dsp` into a `contracts::Effect`.

use crate::catalog::{Curve, EffectSpec, SyncSpec};
use crate::dsp::smooth_coef;
use opendeck_contracts::fx::{Effect, EffectInfo, ParamId, ProcessContext, StereoBufferMut};
use opendeck_contracts::units::SampleRate;

/// Control rate: parameters and filter coefficients update every this many frames.
pub(crate) const CTL: usize = 32;

/// Upper bound on parameters per effect (fixed arrays keep the audio path heap-free).
pub(crate) const MAXP: usize = 8;

/// Smoothing time constant for continuous parameters.
const SMOOTH_TAU_S: f32 = 0.02;

/// Plain parameter values, smoothed at the control rate.
#[derive(Debug, Clone)]
pub(crate) struct Params {
    /// Current (smoothed) plain values, indexed like the catalog.
    pub v: [f32; MAXP],
    target: [f32; MAXP],
    smooth: [bool; MAXP],
    coef: f32,
}

impl Params {
    fn new(spec: &EffectSpec) -> Self {
        let mut s = Self {
            v: [0.0; MAXP],
            target: [0.0; MAXP],
            smooth: [false; MAXP],
            coef: 1.0,
        };
        for (i, p) in spec.params.iter().enumerate().take(MAXP) {
            s.target[i] = p.default;
            s.v[i] = p.default;
            s.smooth[i] = !matches!(p.curve, Curve::Stepped(_));
        }
        s
    }

    fn snap(&mut self) {
        self.v = self.target;
    }

    fn advance(&mut self) {
        for i in 0..MAXP {
            if self.smooth[i] {
                self.v[i] += self.coef * (self.target[i] - self.v[i]);
            } else {
                self.v[i] = self.target[i];
            }
        }
    }

    /// Boolean view of a stepped switch.
    pub fn on(&self, i: usize) -> bool {
        self.v[i] > 0.5
    }
}

/// Musical context for one piece of a block (at most [`CTL`] frames).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    pub sr: f64,
    pub bpm: Option<f64>,
    /// Beat position at the first frame of the piece, if tempo and position are known.
    pub beat: Option<f64>,
    pub division: f64,
    pub frames: usize,
}

impl Ctx {
    /// Length in samples of one division (clamped to the effect's accepted
    /// range), or `fallback_ms` when the tempo is unknown.
    pub fn period_samples(&self, sync: &SyncSpec, fallback_ms: f32) -> f64 {
        match self.bpm {
            Some(bpm) => self.division.clamp(sync.min_beats, sync.max_beats) * 60.0 / bpm * self.sr,
            None => f64::from(fallback_ms) * 0.001 * self.sr,
        }
    }

    /// LFO or gate phase at the start of this piece and its per-sample
    /// increment, for a cycle of `cycle_beats` beats. Locks to the beat grid
    /// when the beat position is known, otherwise free-runs at `fallback_hz`
    /// using `free` as the accumulator.
    pub fn cycle(&self, cycle_beats: f64, fallback_hz: f32, free: &mut f64) -> (f64, f64) {
        let cycle_beats = cycle_beats.max(1e-6);
        let inc = match self.bpm {
            Some(bpm) => bpm / 60.0 / self.sr / cycle_beats,
            None => f64::from(fallback_hz) / self.sr,
        };
        let start = match self.beat {
            Some(b) => (b / cycle_beats).rem_euclid(1.0),
            None => {
                let s = *free;
                *free = (s + inc * self.frames as f64).rem_euclid(1.0);
                s
            }
        };
        (start, inc)
    }
}

/// The per-effect signal processor.
pub(crate) trait Dsp: Send + 'static {
    /// Allocates for `sr`. Called from `Effect::prepare`.
    fn prepare(&mut self, sr: f32, max_block: usize, spec: &'static EffectSpec);
    /// Clears state and re-arms one-shot effects.
    fn reset(&mut self);
    /// Called every [`CTL`] frames before `render`, for coefficients that
    /// depend on parameters only.
    fn update(&mut self, _p: &Params) {}
    /// Writes the wet signal for one piece in place.
    fn render(&mut self, l: &mut [f32], r: &mut [f32], p: &Params, ctx: &Ctx);
    /// Latency in frames.
    fn latency(&self) -> u32 {
        0
    }
}

/// Adapter from [`Dsp`] to `contracts::Effect`.
pub(crate) struct Fx<D: Dsp> {
    info: EffectInfo,
    spec: &'static EffectSpec,
    params: Params,
    dsp: D,
    sr: f64,
    ctl: usize,
    prepared: bool,
}

impl<D: Dsp> Fx<D> {
    pub fn new(spec: &'static EffectSpec, dsp: D) -> Self {
        Self {
            info: spec.info(),
            spec,
            params: Params::new(spec),
            dsp,
            sr: 48_000.0,
            ctl: 0,
            prepared: false,
        }
    }
}

impl<D: Dsp> Effect for Fx<D> {
    fn info(&self) -> &EffectInfo {
        &self.info
    }

    fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize) {
        let sr = sample_rate.0.max(8000) as f32;
        self.sr = f64::from(sr);
        self.params.coef = smooth_coef(CTL, sr, SMOOTH_TAU_S);
        self.params.snap();
        self.dsp.prepare(sr, max_block_frames, self.spec);
        self.dsp.reset();
        self.ctl = 0;
        self.prepared = true;
    }

    fn reset(&mut self) {
        self.dsp.reset();
        self.ctl = 0;
    }

    fn set_parameter(&mut self, id: ParamId, normalized: f32) {
        if !normalized.is_finite() {
            return;
        }
        for (i, d) in self.info.params.iter().enumerate().take(MAXP) {
            if d.id == id {
                self.params.target[i] = d.to_plain(normalized);
                return;
            }
        }
    }

    fn process(&mut self, mut io: StereoBufferMut<'_>, ctx: &ProcessContext) {
        let (l, r) = io.channels();
        if !self.prepared {
            l.fill(0.0);
            r.fill(0.0);
            return;
        }
        for s in l.iter_mut().chain(r.iter_mut()) {
            if !s.is_finite() {
                *s = 0.0;
            }
        }
        let bpm = ctx.bpm.map(|b| b.0).filter(|b| b.is_finite() && *b > 0.0);
        let beat0 = ctx.beat_position.map(|b| b.0).filter(|b| b.is_finite());
        let division = if ctx.beat_division.0.is_finite() && ctx.beat_division.0 > 0.0 {
            ctx.beat_division.0
        } else {
            1.0
        };
        let n = l.len();
        let mut pos = 0;
        while pos < n {
            if self.ctl == 0 {
                self.params.advance();
                self.dsp.update(&self.params);
            }
            let take = (CTL - self.ctl).min(n - pos);
            let beat = match (bpm, beat0) {
                (Some(bpm), Some(b)) => Some(b + pos as f64 * bpm / 60.0 / self.sr),
                _ => None,
            };
            let c = Ctx {
                sr: self.sr,
                bpm,
                beat,
                division,
                frames: take,
            };
            self.dsp.render(
                &mut l[pos..pos + take],
                &mut r[pos..pos + take],
                &self.params,
                &c,
            );
            pos += take;
            self.ctl = (self.ctl + take) % CTL;
        }
    }

    fn latency_frames(&self) -> u32 {
        self.dsp.latency()
    }
}

/// Samples needed for the longest delay of `spec` at [`crate::catalog::MIN_BPM`].
pub(crate) fn max_sync_samples(spec: &EffectSpec, sr: f32) -> usize {
    let beats = spec.sync.map_or(1.0, |s| s.max_beats);
    (beats * 60.0 / crate::catalog::MIN_BPM * f64::from(sr)).ceil() as usize + 8
}
