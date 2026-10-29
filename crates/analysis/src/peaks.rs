//! Streaming waveform peaks with 3-band colour data.
//!
//! One pass over the planar input, no copies. The signal is the stereo mix
//! `(L + R) / 2`. Band split (second-order Butterworth, 12 dB/oct):
//!
//! - low: low-pass at [`LOW_MID_HZ`] (200 Hz)
//! - mid: high-pass at 200 Hz then low-pass at [`MID_HIGH_HZ`] (2.5 kHz)
//! - high: high-pass at 2.5 kHz
//!
//! Each point stores the absolute peak of the full band and of each band over
//! its frames, scaled to `0..=255`. Levels are built bottom-up: the finest
//! level is computed per sample, coarser levels take the max of the finer
//! points, so every level is exact for its own frame grid.

use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use opendeck_contracts::peaks::{PeakLevel, PeakPoint};

use crate::dsp::{check, validate};

/// Frames per point of the zoom levels, finest first (`CONTEXT/analysis.md`).
pub const PEAK_LEVEL_FRAMES: [u32; 3] = [64, 256, 1024];

/// Point count of the overview level. Its frames per point is
/// `ceil(frames / 2048)`, so it has 2048 points (slightly fewer when the
/// division is not exact, one point for a one-frame track).
pub const OVERVIEW_POINTS: usize = 2048;

/// Low/mid crossover, Hz.
pub const LOW_MID_HZ: f64 = 200.0;
/// Mid/high crossover, Hz.
pub const MID_HIGH_HZ: f64 = 2500.0;

/// Frames processed between cancel checks (about 100 ms at 44.1 kHz).
const CHUNK: usize = 4096;

/// Peaks at every zoom level plus the overview.
#[derive(Debug, Default, Clone, Copy)]
pub struct PeaksAnalyzer;

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// RBJ cookbook Butterworth (Q = 1/sqrt 2) low or high pass. Coefficients
    /// are rounded to f32 precision so libm differences cannot change output.
    fn new(freq: f64, rate: f64, high: bool) -> Self {
        let w = std::f64::consts::TAU * freq.min(rate * 0.45) / rate;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = if high {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        };
        let q = |v: f64| f64::from(v as f32);
        Self {
            b0: q(b0 / a0),
            b1: q(b1 / a0),
            b2: q(b2 / a0),
            a1: q(-2.0 * cos / a0),
            a2: q((1.0 - alpha) / a0),
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

#[derive(Clone, Copy, Default)]
struct Acc {
    peak: f32,
    low: f32,
    mid: f32,
    high: f32,
}

impl Acc {
    #[inline]
    fn merge(&mut self, o: &Acc) {
        self.peak = self.peak.max(o.peak);
        self.low = self.low.max(o.low);
        self.mid = self.mid.max(o.mid);
        self.high = self.high.max(o.high);
    }

    fn quantise(&self) -> PeakPoint {
        let q = |v: f32| (v.min(1.0) * 255.0).round() as u8;
        PeakPoint {
            peak: q(self.peak),
            low: q(self.low),
            mid: q(self.mid),
            high: q(self.high),
        }
    }
}

impl Analyzer for PeaksAnalyzer {
    fn id(&self) -> &str {
        "peaks"
    }

    fn version(&self) -> u32 {
        1
    }

    fn features(&self) -> AnalysisFeatures {
        AnalysisFeatures {
            peaks: true,
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
        validate(&audio, 0.0)?;
        let frames = audio.frames();
        let rate = audio.sample_rate.as_f64();
        let base = PEAK_LEVEL_FRAMES[0] as usize;

        let mut lp_low = Biquad::new(LOW_MID_HZ, rate, false);
        let mut hp_mid = Biquad::new(LOW_MID_HZ, rate, true);
        let mut lp_mid = Biquad::new(MID_HIGH_HZ, rate, false);
        let mut hp_high = Biquad::new(MID_HIGH_HZ, rate, true);

        // Finest-level points, built per sample.
        let mut fine: Vec<Acc> = Vec::with_capacity(frames / base + 1);
        let mut cur = Acc::default();
        let mut count = 0usize;
        // Overview: exact frames per point, own accumulator.
        let ov_fpp = frames.div_ceil(OVERVIEW_POINTS).max(1);
        let mut ov: Vec<PeakPoint> = Vec::with_capacity(OVERVIEW_POINTS + 1);
        let mut ov_cur = Acc::default();
        let mut ov_count = 0usize;
        let mut pos = 0usize;
        while pos < frames {
            check(cancel)?;
            let end = (pos + CHUNK).min(frames);
            for i in pos..end {
                let x = match audio.channels {
                    [m] => f64::from(m[i]),
                    [l, r] => 0.5 * (f64::from(l[i]) + f64::from(r[i])),
                    _ => 0.0,
                };
                let low = lp_low.process(x);
                let mid = lp_mid.process(hp_mid.process(x));
                let high = hp_high.process(x);
                let s = Acc {
                    peak: x.abs() as f32,
                    low: low.abs() as f32,
                    mid: mid.abs() as f32,
                    high: high.abs() as f32,
                };
                cur.merge(&s);
                ov_cur.merge(&s);
                ov_count += 1;
                if ov_count == ov_fpp {
                    ov.push(ov_cur.quantise());
                    ov_cur = Acc::default();
                    ov_count = 0;
                }
                count += 1;
                if count == base {
                    fine.push(cur);
                    cur = Acc::default();
                    count = 0;
                }
            }
            pos = end;
            progress(0.9 * pos as f32 / frames as f32);
        }
        if count > 0 {
            fine.push(cur);
        }
        if ov_count > 0 {
            ov.push(ov_cur.quantise());
        }

        let sr = audio.sample_rate.0;
        let rollup = |factor: usize| -> Vec<PeakPoint> {
            fine.chunks(factor)
                .map(|c| {
                    let mut a = Acc::default();
                    c.iter().for_each(|p| a.merge(p));
                    a.quantise()
                })
                .collect()
        };

        let mut levels = Vec::with_capacity(PEAK_LEVEL_FRAMES.len() + 1);
        for &fpp in &PEAK_LEVEL_FRAMES {
            levels.push(PeakLevel {
                sample_rate: sr,
                frames_per_point: fpp,
                points: rollup(fpp as usize / base),
            });
        }
        levels.push(PeakLevel {
            sample_rate: sr,
            frames_per_point: u32::try_from(ov_fpp).unwrap_or(u32::MAX),
            points: ov,
        });
        out.peaks = levels;
        progress(1.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::units::SampleRate;

    fn view<'a>(ch: &'a [&'a [f32]]) -> AudioView<'a> {
        AudioView {
            sample_rate: SampleRate(44_100),
            channels: ch,
        }
    }

    #[test]
    fn levels_and_counts() {
        let n: usize = 44_100 * 3;
        let sig: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let chans: [&[f32]; 1] = [&sig];
        let mut out = AnalysisResult::default();
        PeaksAnalyzer
            .analyze(view(&chans), &mut out, &CancelFlag::new(), &mut |_| {})
            .unwrap();
        assert_eq!(out.peaks.len(), 4);
        assert_eq!(out.peaks[0].frames_per_point, 64);
        assert_eq!(out.peaks[0].points.len(), n.div_ceil(64));
        assert_eq!(out.peaks[2].points.len(), n.div_ceil(1024));
        let ov = &out.peaks[3];
        assert!(ov.points.len() <= OVERVIEW_POINTS && ov.points.len() > 1900);
        let blob = ov.encode();
        assert_eq!(PeakLevel::decode(&blob).unwrap(), *ov);
        // A 0.5 sine reads 127 or 128 at full band.
        assert!(out.peaks[1].points[100].peak.abs_diff(128) <= 2);
    }

    #[test]
    fn bands_follow_frequency() {
        let sr = 44_100.0_f32;
        let tone = |f: f32| -> Vec<f32> {
            (0..44_100)
                .map(|i| 0.8 * (std::f32::consts::TAU * f * i as f32 / sr).sin())
                .collect()
        };
        let probe = |f: f32| {
            let s = tone(f);
            let chans: [&[f32]; 1] = [&s];
            let mut out = AnalysisResult::default();
            PeaksAnalyzer
                .analyze(view(&chans), &mut out, &CancelFlag::new(), &mut |_| {})
                .unwrap();
            out.peaks[2].points[20]
        };
        let lo = probe(60.0);
        assert!(lo.low > lo.mid && lo.low > lo.high);
        let mid = probe(800.0);
        assert!(mid.mid > mid.low && mid.mid > mid.high);
        let hi = probe(8000.0);
        assert!(hi.high > hi.low && hi.high > hi.mid);
    }

    #[test]
    fn cancel_and_invalid() {
        let sig = vec![0.0_f32; 100_000];
        let chans: [&[f32]; 1] = [&sig];
        let cancel = CancelFlag::new();
        cancel.cancel();
        let mut out = AnalysisResult::default();
        assert_eq!(
            PeaksAnalyzer.analyze(view(&chans), &mut out, &cancel, &mut |_| {}),
            Err(AnalysisError::Cancelled)
        );
        let none: [&[f32]; 0] = [];
        assert!(matches!(
            PeaksAnalyzer.analyze(view(&none), &mut out, &CancelFlag::new(), &mut |_| {}),
            Err(AnalysisError::InvalidInput(_))
        ));
    }
}
