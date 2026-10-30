//! Tempo and constant-tempo beat grid.
//!
//! Pipeline:
//!
//! 1. Onset envelope: STFT (1024-sample Hann window, hop 256 at 44.1/48 kHz),
//!    log-compressed band energies, half-wave rectified spectral flux, minus a
//!    local mean, square-rooted. A second, linear low-band flux (below about
//!    200 Hz) is kept for downbeat and octave decisions.
//! 2. Tempo: for each candidate BPM the envelope is folded modulo the beat
//!    period (a comb filter in the phase domain). The score is the mean
//!    envelope mass at the best phase, so octave errors are penalised (half
//!    or double tempo covers only some of the onsets), times a weak log-normal
//!    prior (centre 130 BPM, sigma 1 octave). A 0.25 BPM coarse scan over the
//!    configured range is refined to 0.01 BPM around the best three peaks; the
//!    centre of the score plateau is reported. If low-band (kick) mass half a
//!    beat away is at least 70 % of the on-beat mass and the doubled tempo is
//!    in range, the tempo is doubled (a snare on every second beat otherwise
//!    makes drum and bass read as half tempo).
//! 3. Phase: the folded histogram peak gives the beat phase. The grid is then
//!    refined in the time domain: for up to 128 beats, the steepest rise of
//!    the short-time log energy within +-30 ms of the predicted beat is
//!    located, and a line fitted through the offsets corrects anchor and
//!    period. That removes the STFT window bias and residual tempo error.
//! 4. Downbeat: 4/4 assumed. The beat phase `j` in `0..4` with the largest
//!    mean low-band onset strength becomes beat 0. Ties (within 10 %) keep
//!    the first detected beat. This is a heuristic that fits kick-led music.
//!
//! The anchor is the first downbeat at or after the start of the audio.

use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::units::{Bpm, Seconds};

use crate::dsp::{Stft, check, fill_mono, median, round_to, validate};

/// Tempo analyzer settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoConfig {
    /// Lowest tempo reported, BPM. Octave errors fold into `[min, max]`.
    pub min_bpm: f64,
    /// Highest tempo reported, BPM.
    pub max_bpm: f64,
}

impl Default for TempoConfig {
    /// 70 to 180 BPM.
    fn default() -> Self {
        Self {
            min_bpm: 70.0,
            max_bpm: 180.0,
        }
    }
}

/// BPM and beat grid analyzer.
#[derive(Debug, Default, Clone, Copy)]
pub struct TempoAnalyzer {
    /// Settings.
    pub config: TempoConfig,
}

impl TempoAnalyzer {
    /// Analyzer with a custom BPM range.
    pub fn new(config: TempoConfig) -> Self {
        Self { config }
    }
}

/// Seconds of audio below which tempo is not attempted.
const MIN_SECONDS: f64 = 4.0;
/// Phase-domain smoothing half-width, seconds.
const SMOOTH_S: f64 = 0.010;
/// Prior centre, BPM, and width in octaves.
const PRIOR_CENTRE: f64 = 130.0;
const PRIOR_SIGMA_OCT: f64 = 1.0;
/// Low-band mass ratio between the off-beat and on-beat phase above which
/// the tempo is doubled.
const DOUBLE_RATIO: f64 = 0.7;

struct Onsets {
    /// Rectified spectral flux per frame.
    env: Vec<f32>,
    /// Linear low-band flux per frame.
    low: Vec<f32>,
    win: usize,
    hop: usize,
}

/// STFT window length for a sample rate: 1024 below 64 kHz, else 2048.
fn window_for(rate: u32) -> usize {
    if rate < 64_000 { 1024 } else { 2048 }
}

/// Band edges in bins for the log-spaced onset bands, strictly increasing.
fn band_edges(win: usize, rate: f64) -> Vec<usize> {
    let nyq_bin = win / 2;
    let top = ((16_000.0_f64.min(rate * 0.49) / rate) * win as f64) as usize;
    let top = top.clamp(8, nyq_bin);
    let n_bands = 32;
    let ratio = (top as f64).powf(1.0 / n_bands as f64);
    let mut edges = vec![1usize];
    let mut x = 1.0_f64;
    for _ in 0..n_bands {
        x *= ratio;
        let e = (x.round() as usize)
            .max(edges[edges.len() - 1] + 1)
            .min(top);
        if e > edges[edges.len() - 1] {
            edges.push(e);
        }
    }
    edges
}

fn onsets(
    audio: &AudioView<'_>,
    cancel: &CancelFlag,
    progress: &mut dyn FnMut(f32),
) -> Result<Onsets, AnalysisError> {
    let rate = audio.sample_rate.as_f64();
    let win = window_for(audio.sample_rate.0);
    let hop = win / 4;
    let frames = audio.frames();
    let n = (frames.saturating_sub(win)) / hop + 1;
    let edges = band_edges(win, rate);
    let low_top = ((200.0 / rate * win as f64).round() as usize).max(2);

    let mut stft = Stft::new(win);
    let mut prev = vec![0.0_f32; edges.len() - 1];
    let mut cur = vec![0.0_f32; edges.len() - 1];
    let mut prev_low = 0.0_f32;
    let mut env = Vec::with_capacity(n);
    let mut low = Vec::with_capacity(n);

    for i in 0..n {
        if i % 8 == 0 {
            check(cancel)?;
            progress(0.8 * i as f32 / n as f32);
        }
        stft.run(audio, i * hop);
        for (b, w) in edges.windows(2).enumerate() {
            let e: f32 = (w[0]..w[1])
                .map(|k| {
                    let m = stft.bin(k);
                    m * m
                })
                .sum();
            cur[b] = (1.0 + 1000.0 * e.sqrt()).ln();
        }
        let mut flux = 0.0;
        for (c, p) in cur.iter().zip(&prev) {
            flux += (c - p).max(0.0);
        }
        std::mem::swap(&mut prev, &mut cur);
        env.push(flux / (edges.len() - 1) as f32);

        let ml: f32 = (1..=low_top)
            .map(|k| {
                let m = stft.bin(k);
                m * m
            })
            .sum::<f32>()
            .sqrt();
        low.push((ml - prev_low).max(0.0));
        prev_low = ml;
    }

    // Subtract a local mean (about 0.4 s) and rectify.
    let w = ((0.4 * rate / hop as f64).round() as usize).max(3) | 1;
    detrend(&mut env, w);
    for e in &mut env {
        *e = e.sqrt();
    }
    Ok(Onsets { env, low, win, hop })
}

/// Subtracts a centred moving average of width `w` and clamps at 0.
fn detrend(v: &mut [f32], w: usize) {
    let n = v.len();
    let mut cum = vec![0.0_f64; n + 1];
    for i in 0..n {
        cum[i + 1] = cum[i] + f64::from(v[i]);
    }
    let h = w / 2;
    for (i, x) in v.iter_mut().enumerate() {
        let a = i.saturating_sub(h);
        let b = (i + h + 1).min(n);
        let mean = (cum[b] - cum[a]) / (b - a) as f64;
        *x = (f64::from(*x) - mean).max(0.0) as f32;
    }
}

/// Result of folding the envelope at one period.
struct Fold {
    /// Mean smoothed envelope mass at the best phase, per beat.
    score: f64,
    /// Best phase in frames, `[0, period)`.
    phase: f64,
}

/// Folds `env` modulo `period` (frames) into `bins_per_frame * period` bins
/// and smooths with a circular +-`smooth_frames` box.
fn smoothed_fold(env: &[f32], period: f64, bins_per_frame: f64, smooth_frames: f64) -> Vec<f64> {
    let b = ((period * bins_per_frame).round() as usize).max(8);
    let mut hist = vec![0.0_f64; b];
    let step = 1.0 / period;
    let mut ph = 0.0_f64;
    for &e in env {
        let idx = ((ph * b as f64) as usize).min(b - 1);
        hist[idx] += f64::from(e);
        ph += step;
        if ph >= 1.0 {
            ph -= 1.0;
        }
    }
    let h = ((smooth_frames * bins_per_frame).round() as usize).clamp(1, b / 2 - 1);
    // Circular box sum via a sliding window.
    let mut sum: f64 =
        (0..=h).map(|i| hist[i]).sum::<f64>() + (b - h..b).map(|i| hist[i]).sum::<f64>();
    let mut out = vec![0.0_f64; b];
    for (i, o) in out.iter_mut().enumerate() {
        *o = sum;
        sum += hist[(i + h + 1) % b] - hist[(i + b - h) % b];
    }
    out
}

/// Folds and returns the best phase and per-beat score.
fn fold(env: &[f32], period: f64, bins_per_frame: f64, smooth_frames: f64) -> Fold {
    let sm = smoothed_fold(env, period, bins_per_frame, smooth_frames);
    let b = sm.len();
    let mut best = f64::MIN;
    let mut best_i = 0;
    for (i, &v) in sm.iter().enumerate() {
        if v > best {
            best = v;
            best_i = i;
        }
    }
    let beats = env.len() as f64 / period;
    Fold {
        score: best / beats.max(1.0),
        phase: (best_i as f64 + 0.5) / b as f64 * period,
    }
}

/// Ratio of the smoothed mass half a period away from the best phase to the
/// mass at the best phase, in `[0, 1]`. Near 1 means the pulse repeats at
/// twice the tempo.
fn half_phase_ratio(env: &[f32], period: f64, smooth_frames: f64) -> f64 {
    let sm = smoothed_fold(env, period, 4.0, smooth_frames);
    let b = sm.len();
    let mut best_i = 0;
    for (i, &v) in sm.iter().enumerate() {
        if v > sm[best_i] {
            best_i = i;
        }
    }
    if sm[best_i] <= 0.0 {
        return 0.0;
    }
    let opp = (best_i + b / 2) % b;
    let other = (best_i + b.div_ceil(2)) % b;
    sm[opp].max(sm[other]) / sm[best_i]
}

fn prior(bpm: f64) -> f64 {
    let o = (bpm / PRIOR_CENTRE).log2() / PRIOR_SIGMA_OCT;
    (-0.5 * o * o).exp()
}

/// Estimates the tempo. Returns `(bpm, phase_frames)` or `None` if the
/// envelope carries no rhythm.
fn estimate_tempo(
    o: &Onsets,
    rate: f64,
    cfg: TempoConfig,
    cancel: &CancelFlag,
    progress: &mut dyn FnMut(f32),
) -> Result<Option<(f64, f64)>, AnalysisError> {
    let fps = rate / o.hop as f64; // envelope frames per second
    let smooth = SMOOTH_S * fps;
    let total: f64 = o.env.iter().map(|&e| f64::from(e)).sum();
    if total <= 1e-6 {
        return Ok(None);
    }
    let period_of = |bpm: f64| 60.0 * fps / bpm;

    let (lo, hi) = (cfg.min_bpm.min(cfg.max_bpm), cfg.max_bpm.max(cfg.min_bpm));
    let coarse_step = 0.25;
    let steps = ((hi - lo) / coarse_step).floor() as usize + 1;
    let mut coarse = Vec::with_capacity(steps);
    for s in 0..steps {
        if s % 16 == 0 {
            check(cancel)?;
            progress(0.8 + 0.15 * s as f32 / steps as f32);
        }
        let bpm = lo + s as f64 * coarse_step;
        let f = fold(&o.env, period_of(bpm), 4.0, smooth);
        coarse.push((bpm, f.score * prior(bpm)));
    }

    // Best three local maxima at least 4 % apart.
    let mut order: Vec<usize> = (0..coarse.len()).collect();
    order.sort_by(|&a, &b| coarse[b].1.total_cmp(&coarse[a].1).then(a.cmp(&b)));
    let mut picks: Vec<f64> = Vec::new();
    for &i in &order {
        let bpm = coarse[i].0;
        let is_peak = (i == 0 || coarse[i].1 >= coarse[i - 1].1)
            && (i + 1 == coarse.len() || coarse[i].1 >= coarse[i + 1].1);
        if is_peak && picks.iter().all(|p| (p - bpm).abs() / p > 0.04) {
            picks.push(bpm);
            if picks.len() == 3 {
                break;
            }
        }
    }
    if picks.is_empty() {
        return Ok(None);
    }

    let mut best: Option<(f64, f64, f64)> = None; // (weighted score, bpm, phase)
    for (pi, &c) in picks.iter().enumerate() {
        check(cancel)?;
        progress(0.95 + 0.05 * pi as f32 / picks.len() as f32);
        let fine: Vec<(f64, f64, f64)> = (-30..=30)
            .map(|d| {
                let bpm = c + f64::from(d) * 0.01;
                let f = fold(&o.env, period_of(bpm), 8.0, smooth);
                (bpm, f.score, f.phase)
            })
            .filter(|(bpm, _, _)| *bpm > 0.0)
            .collect();
        let max = fine.iter().map(|f| f.1).fold(f64::MIN, f64::max);
        let arg = fine
            .iter()
            .position(|f| f.1 >= max)
            .unwrap_or(fine.len() / 2);
        // Plateau: contiguous run around the argmax within 3 % of the max.
        let (mut a, mut b) = (arg, arg);
        while a > 0 && fine[a - 1].1 >= 0.97 * max {
            a -= 1;
        }
        while b + 1 < fine.len() && fine[b + 1].1 >= 0.97 * max {
            b += 1;
        }
        let bpm = 0.5 * (fine[a].0 + fine[b].0);
        // Phase at the plateau centre.
        let phase = fold(&o.env, period_of(bpm), 8.0, smooth).phase;
        let weighted = max * prior(bpm);
        if best.is_none_or(|b| weighted > b.0) {
            best = Some((weighted, bpm, phase));
        }
    }
    let Some((_, mut bpm, phase)) = best else {
        return Ok(None);
    };
    // Octave check: kicks (low band) of near-equal strength half a beat away
    // mean the pulse really runs at twice the tempo (for example drum and
    // bass, where a snare on every second beat makes the half tempo win).
    if bpm * 2.0 <= hi && half_phase_ratio(&o.low, period_of(bpm), smooth) >= DOUBLE_RATIO {
        bpm *= 2.0;
    }
    Ok(Some((bpm, phase)))
}

/// Steepest rise of short-time log energy around each predicted beat. Returns
/// `(offset, slope)`: a line `offset + slope * k` (seconds) fitted through
/// the measured onset errors of beat `k`, to add to the anchor and period.
/// `(0, 0)` when too few beats show a clear onset.
fn refine_grid(audio: &AudioView<'_>, first: f64, period_s: f64) -> (f64, f64) {
    const BLOCK: usize = 32;
    const STEP: usize = 16;
    let rate = audio.sample_rate.as_f64();
    let dur = audio.frames() as f64 / rate;
    let half = (0.030 * rate) as usize;
    let nbeats = ((dur - first) / period_s).floor().max(0.0) as usize;
    if nbeats < 4 {
        return (0.0, 0.0);
    }
    let stride = nbeats.div_ceil(128).max(1);
    let mut buf = vec![0.0_f32; 2 * half + BLOCK];
    let mut offsets: Vec<(f64, f64)> = Vec::new();
    let mut k = 0;
    while k < nbeats {
        let centre = ((first + k as f64 * period_s) * rate).round() as isize;
        let start = centre - half as isize;
        if start >= 0 {
            fill_mono(audio, start as usize, &mut buf);
            let blocks = (buf.len() - BLOCK) / STEP + 1;
            let e: Vec<f64> = (0..blocks)
                .map(|j| {
                    buf[j * STEP..j * STEP + BLOCK]
                        .iter()
                        .map(|&x| f64::from(x) * f64::from(x))
                        .sum()
                })
                .collect();
            let emax = e.iter().copied().fold(0.0, f64::max);
            if emax > 1e-12 {
                let eps = 1e-4 * emax;
                let mut best = (0.0_f64, 0usize);
                for j in 0..blocks - 1 {
                    let r = (e[j + 1] + eps).ln() - (e[j] + eps).ln();
                    if r > best.0 {
                        best = (r, j);
                    }
                }
                if best.0 > 0.7 {
                    // Rise between block j and j+1: onset at the start of j+1
                    // plus half a step (calibrated on the synthetic set).
                    let at = (best.1 * STEP + BLOCK) as f64;
                    offsets.push((k as f64, (at - half as f64) / rate));
                }
            }
        }
        k += stride;
    }
    if offsets.len() < 8 {
        return (0.0, 0.0);
    }
    let mut values: Vec<f64> = offsets.iter().map(|p| p.1).collect();
    let m = median(&mut values).clamp(-0.030, 0.030);
    // Least squares line through the inliers (within 8 ms of the median).
    let inliers: Vec<&(f64, f64)> = offsets.iter().filter(|p| (p.1 - m).abs() < 0.008).collect();
    if inliers.len() < 8 {
        return (m, 0.0);
    }
    let n = inliers.len() as f64;
    let (sx, sy) = inliers
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (sx / n, sy / n);
    let (mut sxx, mut sxy) = (0.0, 0.0);
    for p in &inliers {
        sxx += (p.0 - mx) * (p.0 - mx);
        sxy += (p.0 - mx) * (p.1 - my);
    }
    let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    // Ignore implausible tempo corrections (more than 0.5 %).
    if slope.abs() > 0.005 * period_s {
        return (m, 0.0);
    }
    (my - slope * mx, slope)
}

/// Picks which of 4 consecutive beats is the downbeat; returns its index.
fn pick_downbeat(o: &Onsets, rate: f64, first: f64, period_s: f64, dur: f64) -> usize {
    let nbeats = ((dur - first) / period_s).floor().max(0.0) as usize;
    let mut sums = [0.0_f64; 4];
    let mut counts = [0usize; 4];
    for k in 0..nbeats {
        let t = first + k as f64 * period_s;
        let idx = ((t * rate - o.win as f64 * 0.5) / o.hop as f64).round() as isize;
        let a = (idx - 2).max(0) as usize;
        let b = ((idx + 3).max(0) as usize).min(o.low.len());
        if a >= b {
            continue;
        }
        let s = o.low[a..b].iter().copied().fold(0.0_f32, f32::max);
        sums[k % 4] += f64::from(s);
        counts[k % 4] += 1;
    }
    let mean = |j: usize| sums[j] / counts[j].max(1) as f64;
    let (mut best, mut best_v) = (0, mean(0));
    for j in 1..4 {
        if mean(j) > best_v * 1.1 {
            best = j;
            best_v = mean(j);
        }
    }
    best
}

impl Analyzer for TempoAnalyzer {
    fn id(&self) -> &str {
        "tempo"
    }

    fn version(&self) -> u32 {
        1
    }

    fn features(&self) -> AnalysisFeatures {
        AnalysisFeatures {
            tempo: true,
            ..AnalysisFeatures::default()
        }
    }

    fn analyze(
        &self,
        audio: AudioView<'_>,
        out: &mut AnalysisResult,
        cancel: &CancelFlag,
        progress: &mut dyn FnMut(f32),
    ) -> Result<(), AnalysisError> {
        validate(&audio, MIN_SECONDS)?;
        let rate = audio.sample_rate.as_f64();
        let dur = audio.frames() as f64 / rate;
        let o = onsets(&audio, cancel, progress)?;
        let Some((bpm, phase)) = estimate_tempo(&o, rate, self.config, cancel, progress)? else {
            out.bpm = None;
            out.beat_grid = None;
            return Ok(());
        };
        let mut period_s = 60.0 / bpm;
        // Envelope frame i is centred at (i * hop + win / 2) / rate.
        let mut first = (phase * o.hop as f64 + o.win as f64 * 0.5) / rate;
        first = first.rem_euclid(period_s);
        // A beat just before the start belongs to the audio's first instant.
        if first > period_s - 0.03 {
            first -= period_s;
        }
        let (offset, slope) = refine_grid(&audio, first, period_s);
        first += offset;
        period_s += slope;
        let bpm = 60.0 / period_s;
        if first < -0.03 {
            first += period_s;
        }
        let j = pick_downbeat(&o, rate, first, period_s, dur);
        let anchor = first + j as f64 * period_s;

        let bpm = round_to(bpm, 2);
        let anchor = round_to(anchor, 4);
        let grid = BeatGrid::new(Seconds(anchor), Bpm(bpm), 4)
            .map_err(|e| AnalysisError::Failed(e.to_string()))?;
        out.bpm = Some(Bpm(bpm));
        out.beat_grid = Some(grid);
        progress(1.0);
        Ok(())
    }
}
