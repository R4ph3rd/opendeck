//! The mixer: per-channel strips (trim, 3-band EQ, filter, PFL tap, channel
//! fader), crossfader, master level and the headphone bus.
//!
//! Everything here runs on the audio thread and never allocates after
//! [`Mixer::new`]. Parameters arrive as targets and are reached with a linear
//! ramp over the next block, so nothing steps (no zipper noise).
//!
//! # Signal flow
//!
//! ```text
//! sampler bus -> master bus (and cue bus with sampler PFL), before the master FX
//! deck -> trim -> EQ -> filter -> FX insert -+-> (PFL on) cue bus
//!                                            '-> channel fader -> meter -> crossfader side -> master bus
//! master bus -> master level -> master out (and its meter)
//! cue bus, master out -> headphone blend or split cue -> headphone level -> headphone out
//! ```
//!
//! # Product choices (not external specs)
//!
//! - EQ: isolator style. Linkwitz-Riley 8th order crossovers at
//!   [`EQ_LOW_MID_HZ`] and [`EQ_MID_HIGH_HZ`]. The low band passes through an
//!   all-pass at the upper crossover so that unity gains sum flat. Each band
//!   gain runs from 0 (kill) to [`EQ_MAX_GAIN_DB`] linear in amplitude. With all
//!   three gains at unity the EQ is bypassed bit-exactly after a one block
//!   crossfade.
//! - Filter: one knob. A dead zone of +/- [`FILTER_DEAD_ZONE`] around the
//!   centre bypasses it. Left of centre is a low pass, right a high pass, both
//!   2-pole state-variable filters with a mild resonance ([`FILTER_Q`]). The
//!   cutoff sweeps exponentially.
//! - Channel fader: linear in amplitude.
//! - Headphone blend: equal power between cue (0) and master (1). Split cue
//!   ignores the blend: cue mono on the left, master mono on the right.
//! - Headphones tap the master after the master level.

use opendeck_contracts::mixer::{
    CrossfaderAssign, CrossfaderCurve, EQ_MAX_GAIN_DB, TRIM_MAX_GAIN_DB,
};
use opendeck_contracts::telemetry::StereoMeter;
use opendeck_contracts::units::db_to_gain;
use opendeck_contracts::{EngineCommand, MAX_DECKS};

use crate::dsp::{Lr, Pass, Svf, SvfCoeffs};
use crate::fx::FxHost;
use crate::meter::MeterAcc;

/// Lower EQ crossover, Hz.
pub const EQ_LOW_MID_HZ: f64 = 200.0;
/// Upper EQ crossover, Hz.
pub const EQ_MID_HIGH_HZ: f64 = 2_500.0;
/// Half width of the filter bypass zone around the knob centre.
pub const FILTER_DEAD_ZONE: f32 = 0.02;
/// Filter resonance (Q). Butterworth is 0.707; 1.2 adds about 1.6 dB at the cutoff.
pub const FILTER_Q: f64 = 1.2;
/// Low pass cutoff at the dead zone edge, Hz.
const LP_MAX_HZ: f64 = 20_000.0;
/// Low pass cutoff with the knob fully left, Hz.
const LP_MIN_HZ: f64 = 100.0;
/// High pass cutoff at the dead zone edge, Hz.
const HP_MIN_HZ: f64 = 20.0;
/// High pass cutoff with the knob fully right, Hz.
const HP_MAX_HZ: f64 = 8_000.0;
/// Frames between filter coefficient updates.
const FILTER_STEP: usize = 32;

/// Linear gain that moves to its target over one block.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ramp {
    current: f32,
    target: f32,
}

impl Ramp {
    pub(crate) fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
        }
    }

    /// Sets the target; non-finite values are ignored, negatives clamp to 0.
    pub(crate) fn set(&mut self, value: f32) {
        if value.is_finite() {
            self.target = value.max(0.0);
        }
    }

    /// True when the ramp is parked at `value`.
    pub(crate) fn is_at(&self, value: f32) -> bool {
        self.current == value && self.target == value
    }

    /// Returns `(start, step)` for an `n` frame block and settles on the target.
    /// The gain of frame `i` (0-based) is `start + step * (i + 1)`.
    pub(crate) fn advance(&mut self, n: usize) -> (f32, f32) {
        let start = self.current;
        let step = (self.target - start) / n.max(1) as f32;
        self.current = self.target;
        (start, step)
    }
}

/// One side (left or right) of the 3-band EQ.
#[derive(Debug, Clone, Copy)]
struct EqSide {
    lo1: Lr,
    hi1: Lr,
    lo2: Lr,
    hi2: Lr,
    ap_lo: Lr,
    ap_hi: Lr,
}

impl EqSide {
    fn new(sample_rate: f64) -> Self {
        let lp = |fc| Lr::new(Pass::Low, fc, sample_rate);
        let hp = |fc| Lr::new(Pass::High, fc, sample_rate);
        Self {
            lo1: lp(EQ_LOW_MID_HZ),
            hi1: hp(EQ_LOW_MID_HZ),
            lo2: lp(EQ_MID_HIGH_HZ),
            hi2: hp(EQ_MID_HIGH_HZ),
            ap_lo: lp(EQ_MID_HIGH_HZ),
            ap_hi: hp(EQ_MID_HIGH_HZ),
        }
    }

    /// Splits `x` into `[low, mid, high]`. Their sum has flat magnitude.
    #[inline]
    fn split(&mut self, x: f64) -> [f64; 3] {
        let low = self.lo1.tick(x);
        let rest = self.hi1.tick(x);
        let mid = self.lo2.tick(rest);
        let high = self.hi2.tick(rest);
        // Match the low band's phase to the other two (they went through the
        // upper crossover, it did not).
        let low = self.ap_lo.tick(low) + self.ap_hi.tick(low);
        [low, mid, high]
    }
}

/// One channel strip.
#[derive(Debug, Clone)]
pub(crate) struct Strip {
    sample_rate: f64,
    trim: Ramp,
    eq_gain: [Ramp; 3],
    eq_wet: Ramp,
    eq: [EqSide; 2],
    filter_amount: f32,
    filter_current: f32,
    svf: [Svf; 2],
    fader: Ramp,
    crossfade: Ramp,
    assign: CrossfaderAssign,
    pfl: bool,
    pub(crate) meter: MeterAcc,
}

impl Strip {
    fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            trim: Ramp::new(1.0),
            eq_gain: [Ramp::new(1.0); 3],
            eq_wet: Ramp::new(0.0),
            eq: [EqSide::new(sample_rate); 2],
            filter_amount: 0.0,
            filter_current: 0.0,
            svf: [Svf::default(); 2],
            fader: Ramp::new(1.0),
            crossfade: Ramp::new(1.0),
            assign: CrossfaderAssign::Thru,
            pfl: false,
            meter: MeterAcc::default(),
        }
    }

    /// First half of the strip: trim, EQ and filter, in place on `buf`
    /// (interleaved stereo). The FX insert chain runs after this.
    pub(crate) fn pre_fx(&mut self, buf: &mut [f32]) {
        let n = buf.len() / 2;
        self.apply_trim(buf, n);
        self.apply_eq(buf, n);
        self.apply_filter(buf, n);
    }

    /// Second half: PFL tap, channel fader, meter and crossfader side. Adds the
    /// post-fader signal to `master` and, with PFL on, the pre-fader signal to
    /// `cue`. `buf` is left holding the post-fader signal. `send_tap`, when
    /// given, receives what this channel adds to `master` (post fader and
    /// crossfader) for a post-fader FX send.
    pub(crate) fn post_fx(
        &mut self,
        buf: &mut [f32],
        master: &mut [f32],
        cue: &mut [f32],
        mut send_tap: Option<&mut [f32]>,
    ) {
        let n = buf.len() / 2;
        if self.pfl {
            for (c, s) in cue.iter_mut().zip(buf.iter()) {
                *c += *s;
            }
        }
        let (start, step) = self.fader.advance(n);
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let g = start + step * (i + 1) as f32;
            frame[0] *= g;
            frame[1] *= g;
        }
        self.meter.add(buf);
        let (start, step) = self.crossfade.advance(n);
        for (i, (m, s)) in master
            .chunks_exact_mut(2)
            .zip(buf.chunks_exact(2))
            .enumerate()
        {
            let g = start + step * (i + 1) as f32;
            m[0] += s[0] * g;
            m[1] += s[1] * g;
            if let Some(t) = send_tap
                .as_deref_mut()
                .and_then(|t| t.get_mut(i * 2..i * 2 + 2))
            {
                t[0] = s[0] * g;
                t[1] = s[1] * g;
            }
        }
    }

    fn apply_trim(&mut self, buf: &mut [f32], n: usize) {
        let (start, step) = self.trim.advance(n);
        if start == 1.0 && step == 0.0 {
            return;
        }
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let g = start + step * (i + 1) as f32;
            frame[0] *= g;
            frame[1] *= g;
        }
    }

    fn apply_eq(&mut self, buf: &mut [f32], n: usize) {
        let flat = self.eq_gain.iter().all(|g| g.is_at(1.0));
        self.eq_wet.set(if flat { 0.0 } else { 1.0 });
        let (w0, wstep) = self.eq_wet.advance(n);
        let ramps = self.eq_gain.map(|mut g| g.advance(n));
        // Settled bypass: the filters keep running so they are warm when the
        // EQ engages, but the output is the untouched input.
        let bypass = w0 == 0.0 && wstep == 0.0;
        for (i, frame) in buf.chunks_exact_mut(2).enumerate() {
            let t = (i + 1) as f32;
            let w = w0 + wstep * t;
            let mut wet = [0.0_f32; 2];
            for ch in 0..2 {
                let x = f64::from(frame[ch]);
                let bands = self.eq[ch].split(x);
                let mut sum = 0.0;
                for (band, (start, step)) in bands.iter().zip(ramps.iter()) {
                    sum += band * f64::from(start + step * t);
                }
                wet[ch] = sum as f32;
            }
            if !bypass {
                frame[0] += (wet[0] - frame[0]) * w;
                frame[1] += (wet[1] - frame[1]) * w;
            }
        }
        // `ramps` were taken from copies; settle the real ones.
        for g in &mut self.eq_gain {
            g.advance(n);
        }
    }

    fn apply_filter(&mut self, buf: &mut [f32], n: usize) {
        let a0 = self.filter_current;
        let a1 = self.filter_amount;
        self.filter_current = a1;
        let mut start = 0;
        while start < n {
            let m = FILTER_STEP.min(n - start);
            let mid = (start + m / 2) as f32 / n as f32;
            let amount = a0 + (a1 - a0) * mid;
            let range = start * 2..(start + m) * 2;
            start += m;
            let Some(chunk) = buf.get_mut(range) else {
                continue;
            };
            if amount.abs() < FILTER_DEAD_ZONE {
                for s in &mut self.svf {
                    s.reset();
                }
                continue;
            }
            let x = f64::from((amount.abs() - FILTER_DEAD_ZONE) / (1.0 - FILTER_DEAD_ZONE))
                .clamp(0.0, 1.0);
            let low_pass = amount < 0.0;
            let fc = if low_pass {
                LP_MAX_HZ * (LP_MIN_HZ / LP_MAX_HZ).powf(x)
            } else {
                HP_MIN_HZ * (HP_MAX_HZ / HP_MIN_HZ).powf(x)
            };
            let c = SvfCoeffs::new(fc.min(0.45 * self.sample_rate), FILTER_Q, self.sample_rate);
            for frame in chunk.chunks_exact_mut(2) {
                for (s, svf) in frame.iter_mut().zip(self.svf.iter_mut()) {
                    let (lp, hp) = svf.tick(f64::from(*s), &c);
                    *s = if low_pass { lp } else { hp } as f32;
                }
            }
        }
    }
}

/// All mixer state.
pub(crate) struct Mixer {
    strips: [Strip; MAX_DECKS],
    crossfader: f32,
    curve: CrossfaderCurve,
    master: Ramp,
    headphone_level: Ramp,
    headphone_mix: f32,
    cue_gain: Ramp,
    master_gain: Ramp,
    split: bool,
    split_mix: Ramp,
    pub(crate) master_meter: MeterAcc,
    pub(crate) headphone_meter: MeterAcc,
    cue_bus: Vec<f32>,
}

impl Mixer {
    /// Allocates the cue bus. Not real-time safe.
    pub(crate) fn new(sample_rate: f64, max_block_frames: usize) -> Self {
        Self {
            strips: std::array::from_fn(|_| Strip::new(sample_rate)),
            crossfader: 0.0,
            curve: CrossfaderCurve::default(),
            master: Ramp::new(1.0),
            headphone_level: Ramp::new(1.0),
            headphone_mix: 0.0,
            cue_gain: Ramp::new(1.0),
            master_gain: Ramp::new(0.0),
            split: false,
            split_mix: Ramp::new(0.0),
            master_meter: MeterAcc::default(),
            headphone_meter: MeterAcc::default(),
            cue_bus: vec![0.0; max_block_frames.max(1) * 2],
        }
    }

    /// Applies a mixer command. Returns false for any other command.
    pub(crate) fn apply(&mut self, command: EngineCommand) -> bool {
        use EngineCommand as C;
        match command {
            C::SetTrim { deck, gain } => {
                if let Some(s) = self.strips.get_mut(deck.index())
                    && gain.0.is_finite()
                {
                    s.trim.set(gain.0.min(db_to_gain(TRIM_MAX_GAIN_DB)));
                }
            }
            C::SetEq { deck, band, gain } => {
                if let Some(g) = self
                    .strips
                    .get_mut(deck.index())
                    .and_then(|s| s.eq_gain.get_mut(band.index()))
                    && gain.0.is_finite()
                {
                    g.set(gain.0.min(db_to_gain(EQ_MAX_GAIN_DB)));
                }
            }
            C::SetFilter { deck, amount } => {
                if let Some(s) = self.strips.get_mut(deck.index())
                    && amount.is_finite()
                {
                    s.filter_amount = amount.clamp(-1.0, 1.0);
                }
            }
            C::SetChannelFader { deck, position } => {
                if let Some(s) = self.strips.get_mut(deck.index())
                    && position.is_finite()
                {
                    s.fader.set(position.clamp(0.0, 1.0));
                }
            }
            C::SetPfl { deck, on } => {
                if let Some(s) = self.strips.get_mut(deck.index()) {
                    s.pfl = on;
                }
            }
            C::SetCrossfaderAssign { deck, assign } => {
                if let Some(s) = self.strips.get_mut(deck.index()) {
                    s.assign = assign;
                }
            }
            C::SetCrossfader { position } => {
                if position.is_finite() {
                    self.crossfader = position.clamp(-1.0, 1.0);
                }
            }
            C::SetCrossfaderCurve { curve } => self.curve = curve,
            C::SetMasterLevel { gain } => self.master.set(gain.0),
            C::SetHeadphoneLevel { gain } => self.headphone_level.set(gain.0),
            C::SetHeadphoneMix { mix } => {
                if mix.is_finite() {
                    self.headphone_mix = mix.clamp(0.0, 1.0);
                }
            }
            C::SetSplitCue { on } => self.split = on,
            _ => return false,
        }
        true
    }

    /// Prepares a block: clears the cue bus and sets the crossfader targets.
    pub(crate) fn begin_block(&mut self, frames: usize) {
        if let Some(bus) = self.cue_bus.get_mut(..frames * 2) {
            bus.fill(0.0);
        }
        let (a, b) = self.curve.gains(self.crossfader);
        for strip in &mut self.strips {
            let g = match strip.assign {
                CrossfaderAssign::A => a.0,
                CrossfaderAssign::B => b.0,
                CrossfaderAssign::Thru => 1.0,
            };
            strip.crossfade.set(g);
        }
    }

    /// Runs one deck's block through its strip into `master` (accumulating).
    /// The deck's FX insert chain sits between EQ/filter and the PFL tap and
    /// fader; `fx` also takes the pre and post-fader sends.
    pub(crate) fn process_deck(
        &mut self,
        index: usize,
        buf: &mut [f32],
        master: &mut [f32],
        fx: &mut FxHost,
    ) {
        let frames = buf.len() / 2;
        let (Some(strip), Some(cue)) = (
            self.strips.get_mut(index),
            self.cue_bus.get_mut(..frames * 2),
        ) else {
            return;
        };
        strip.pre_fx(buf);
        fx.insert_deck(index, buf);
        fx.send_pre(index, buf);
        let tap = fx.post_tap(index, frames);
        let tapped = tap.is_some();
        strip.post_fx(buf, master, cue, tap);
        if tapped {
            fx.send_post(index, frames);
        }
    }

    /// Adds the sampler bus to `master` and, with `pfl`, to the cue bus.
    /// Call after the decks and sends, before the master FX.
    pub(crate) fn add_sampler(&mut self, bus: &[f32], master: &mut [f32], pfl: bool) {
        for (m, s) in master.iter_mut().zip(bus) {
            *m += *s;
        }
        if pfl && let Some(cue) = self.cue_bus.get_mut(..bus.len()) {
            for (c, s) in cue.iter_mut().zip(bus) {
                *c += *s;
            }
        }
    }

    /// Post-fader meter of channel `index`, taken for a telemetry snapshot.
    pub(crate) fn take_channel_meter(&mut self, index: usize) -> StereoMeter {
        self.strips
            .get_mut(index)
            .map_or_else(StereoMeter::default, |s| s.meter.take())
    }

    /// Applies the master level to `master` (in place) and renders the
    /// headphone bus into `headphones`. Both are interleaved stereo.
    pub(crate) fn finish(&mut self, master: &mut [f32], headphones: &mut [f32]) {
        let n = master.len() / 2;
        let (start, step) = self.master.advance(n);
        for (i, frame) in master.chunks_exact_mut(2).enumerate() {
            let g = start + step * (i + 1) as f32;
            for s in frame {
                let v = *s * g;
                *s = if v.is_finite() { v } else { 0.0 };
            }
        }
        self.master_meter.add(master);

        let angle = self.headphone_mix * std::f32::consts::FRAC_PI_2;
        self.cue_gain.set(angle.cos());
        self.master_gain.set(angle.sin());
        self.split_mix.set(if self.split { 1.0 } else { 0.0 });
        let (cue0, cue_step) = self.cue_gain.advance(n);
        let (mas0, mas_step) = self.master_gain.advance(n);
        let (spl0, spl_step) = self.split_mix.advance(n);
        let (lvl0, lvl_step) = self.headphone_level.advance(n);
        let Some(cue) = self.cue_bus.get(..n * 2) else {
            headphones.fill(0.0);
            return;
        };
        for (i, ((out, c), m)) in headphones
            .chunks_exact_mut(2)
            .zip(cue.chunks_exact(2))
            .zip(master.chunks_exact(2))
            .enumerate()
        {
            let t = (i + 1) as f32;
            let (gc, gm) = (cue0 + cue_step * t, mas0 + mas_step * t);
            let (s, level) = (spl0 + spl_step * t, lvl0 + lvl_step * t);
            let split_l = 0.5 * (c[0] + c[1]);
            let split_r = 0.5 * (m[0] + m[1]);
            let l = (c[0] * gc + m[0] * gm) * (1.0 - s) + split_l * s;
            let r = (c[1] * gc + m[1] * gm) * (1.0 - s) + split_r * s;
            out[0] = guard(l * level);
            out[1] = guard(r * level);
        }
        self.headphone_meter.add(headphones);
    }
}

#[inline]
fn guard(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_reaches_target_and_ignores_nan() {
        let mut r = Ramp::new(1.0);
        r.set(f32::NAN);
        assert_eq!(r.advance(4), (1.0, 0.0));
        r.set(0.0);
        let (start, step) = r.advance(4);
        assert_eq!(start, 1.0);
        assert!((start + step * 4.0).abs() < 1e-6);
    }
}
