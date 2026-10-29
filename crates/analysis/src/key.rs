//! Musical key from a tuning-corrected chroma vector and Krumhansl-Schmuckler
//! profile matching.
//!
//! 1. STFT with an 8192-sample Hann window, hop 4096 (about 5 Hz bins at
//!    44.1 kHz), over 55 Hz to 4.2 kHz.
//! 2. Spectral peaks (local maxima above 5 % of the frame maximum) are
//!    refined by parabolic interpolation on log magnitude and accumulated,
//!    with `sqrt(magnitude / frame max)` weight, into a 36-bin pitch
//!    histogram (3 bins per semitone).
//! 3. The tuning offset (-1, 0, +1 third of a semitone) is the residue class
//!    with the most mass; the histogram folds to 12 pitch classes around it.
//! 4. Pearson correlation against the 24 rotated Krumhansl-Kessler probe-tone
//!    profiles (Krumhansl and Kessler, Psychological Review 89, 1982); the
//!    best is the key.

use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use opendeck_contracts::key::{KeyMode, MusicalKey};

use crate::dsp::{Stft, check, validate};

/// Krumhansl-Kessler major profile, C at index 0.
const MAJOR: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
/// Krumhansl-Kessler minor profile, C at index 0.
const MINOR: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

const MIN_SECONDS: f64 = 3.0;
const MIN_HZ: f64 = 55.0;
const MAX_HZ: f64 = 4200.0;

/// Key analyzer.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyAnalyzer;

fn pearson(a: &[f64; 12], b: &[f64; 12]) -> f64 {
    let ma = a.iter().sum::<f64>() / 12.0;
    let mb = b.iter().sum::<f64>() / 12.0;
    let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
    for i in 0..12 {
        let (x, y) = (a[i] - ma, b[i] - mb);
        num += x * y;
        da += x * x;
        db += y * y;
    }
    if da <= 0.0 || db <= 0.0 {
        0.0
    } else {
        num / (da * db).sqrt()
    }
}

/// Best key for a 12-bin chroma vector (C = 0), or `None` if it is flat.
fn best_key(chroma: &[f64; 12]) -> Option<MusicalKey> {
    if chroma.iter().sum::<f64>() <= 0.0 {
        return None;
    }
    let mut best = (f64::MIN, 0u8, KeyMode::Major);
    for tonic in 0..12u8 {
        for (profile, mode) in [(&MAJOR, KeyMode::Major), (&MINOR, KeyMode::Minor)] {
            let mut rotated = [0.0; 12];
            for (i, r) in rotated.iter_mut().enumerate() {
                *r = profile[(i + 12 - usize::from(tonic)) % 12];
            }
            let c = pearson(chroma, &rotated);
            if c > best.0 {
                best = (c, tonic, mode);
            }
        }
    }
    MusicalKey::new(best.1, best.2)
}

impl Analyzer for KeyAnalyzer {
    fn id(&self) -> &str {
        "key"
    }

    fn version(&self) -> u32 {
        1
    }

    fn features(&self) -> AnalysisFeatures {
        AnalysisFeatures {
            key: true,
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
        let win = if rate < 64_000.0 { 8192 } else { 16_384 };
        let hop = win / 2;
        let frames = audio.frames();
        let n = frames.saturating_sub(win) / hop + 1;
        let lo = ((MIN_HZ * win as f64 / rate).ceil() as usize).max(2);
        let hi = ((MAX_HZ.min(rate * 0.45) * win as f64 / rate) as usize).min(win / 2 - 2);

        let mut stft = Stft::new(win);
        let mut hist = [0.0_f64; 36];
        let mut mags = vec![0.0_f32; win / 2 + 1];
        for i in 0..n {
            check(cancel)?;
            progress(i as f32 / n as f32);
            stft.run(&audio, i * hop);
            let mut max = 0.0_f32;
            for (k, m) in mags.iter_mut().enumerate().take(hi + 2).skip(lo - 1) {
                *m = stft.bin(k);
                max = max.max(*m);
            }
            if max < 1e-5 {
                continue;
            }
            for k in lo..=hi {
                let m = mags[k];
                if m < 0.05 * max || m < mags[k - 1] || m <= mags[k + 1] {
                    continue;
                }
                // Parabolic interpolation on log magnitude.
                let (a, b, c) = (
                    f64::from(mags[k - 1]).max(1e-12).ln(),
                    f64::from(m).ln(),
                    f64::from(mags[k + 1]).max(1e-12).ln(),
                );
                let denom = a - 2.0 * b + c;
                let delta = if denom.abs() > 1e-12 {
                    (0.5 * (a - c) / denom).clamp(-0.5, 0.5)
                } else {
                    0.0
                };
                let freq = (k as f64 + delta) * rate / win as f64;
                let pitch = 69.0 + 12.0 * (freq / 440.0).log2();
                let bin = ((pitch * 3.0).round() as i64).rem_euclid(36) as usize;
                hist[bin] += f64::from(m / max).sqrt();
            }
        }

        // Tuning: residue class (mod 3) with the most mass.
        let mut res = [0.0_f64; 3];
        for (b, &v) in hist.iter().enumerate() {
            res[b % 3] += v;
        }
        let mut r = 0usize;
        for c in 1..3 {
            if res[c] > res[r] {
                r = c;
            }
        }
        // Signed offset of the semitone centres in third-semitone bins.
        let r: i32 = if r == 2 { -1 } else { r as i32 };
        let mut chroma = [0.0_f64; 12];
        for (b, &v) in hist.iter().enumerate() {
            // Bins r-1, r, r+1 around a semitone centre fold to that semitone.
            let semitone = ((b as i32 - r + 1 + 36) / 3) as usize % 12;
            chroma[semitone] += v;
        }
        out.key = best_key(&chroma);
        progress(1.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_matches_itself() {
        for tonic in 0..12u8 {
            let mut c = [0.0; 12];
            for i in 0..12 {
                c[(i + usize::from(tonic)) % 12] = MAJOR[i];
            }
            let k = best_key(&c).unwrap();
            assert_eq!((k.tonic(), k.mode()), (tonic, KeyMode::Major));
            for i in 0..12 {
                c[(i + usize::from(tonic)) % 12] = MINOR[i];
            }
            let k = best_key(&c).unwrap();
            assert_eq!((k.tonic(), k.mode()), (tonic, KeyMode::Minor));
        }
        assert!(best_key(&[0.0; 12]).is_none());
    }
}
