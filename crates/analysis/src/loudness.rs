//! EBU R128 integrated loudness and true peak via `ebur128` (MIT).

use ebur128::{EbuR128, Mode};
use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag, Loudness,
};

use crate::dsp::{check, round_to, validate};

/// Reported when the programme is silent (everything gated out).
pub const SILENCE_LUFS: f64 = -70.0;
/// Reported true peak for digital silence, dBTP.
pub const SILENCE_DBTP: f64 = -120.0;

/// Loudness analyzer.
#[derive(Debug, Default, Clone, Copy)]
pub struct LoudnessAnalyzer;

impl Analyzer for LoudnessAnalyzer {
    fn id(&self) -> &str {
        "loudness"
    }

    fn version(&self) -> u32 {
        1
    }

    fn features(&self) -> AnalysisFeatures {
        AnalysisFeatures {
            loudness: true,
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
        let channels = audio.channels.len() as u32;
        let fail = |e: ebur128::Error| AnalysisError::Failed(e.to_string());
        let mut meter =
            EbuR128::new(channels, audio.sample_rate.0, Mode::I | Mode::TRUE_PEAK).map_err(fail)?;

        // 100 ms chunks keep cancellation responsive.
        let chunk = (audio.sample_rate.0 as usize / 10).max(1);
        let mut pos = 0;
        while pos < frames {
            check(cancel)?;
            let end = (pos + chunk).min(frames);
            let slices: Vec<&[f32]> = audio.channels.iter().map(|c| &c[pos..end]).collect();
            meter.add_frames_planar_f32(&slices).map_err(fail)?;
            pos = end;
            progress(pos as f32 / frames as f32);
        }

        let lufs = meter.loudness_global().map_err(fail)?;
        let mut peak = 0.0_f64;
        for c in 0..channels {
            peak = peak.max(meter.true_peak(c).map_err(fail)?);
        }
        let integrated = if lufs.is_finite() { lufs } else { SILENCE_LUFS };
        let dbtp = if peak > 0.0 {
            20.0 * peak.log10()
        } else {
            SILENCE_DBTP
        };
        out.loudness = Some(Loudness {
            integrated_lufs: round_to(integrated, 2),
            true_peak_dbtp: round_to(dbtp.max(SILENCE_DBTP), 2),
        });
        progress(1.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::units::SampleRate;

    #[test]
    fn calibration_tone_reads_expected_loudness() {
        // EBU Tech 3341: a 1 kHz sine at -23 dBFS in both channels reads
        // -23 LUFS, so -20 dBFS in both channels reads -20 LUFS.
        let sr = 48_000;
        let amp = 10f32.powf(-20.0 / 20.0);
        let s: Vec<f32> = (0..sr * 5)
            .map(|i| amp * (std::f32::consts::TAU * 1000.0 * i as f32 / sr as f32).sin())
            .collect();
        let chans: [&[f32]; 2] = [&s, &s];
        let mut out = AnalysisResult::default();
        LoudnessAnalyzer
            .analyze(
                AudioView {
                    sample_rate: SampleRate(sr as u32),
                    channels: &chans,
                },
                &mut out,
                &CancelFlag::new(),
                &mut |_| {},
            )
            .unwrap();
        let l = out.loudness.unwrap();
        assert!((l.integrated_lufs - (-20.0)).abs() < 0.2, "{l:?}");
        assert!((l.true_peak_dbtp - (-20.0)).abs() < 0.3, "{l:?}");
    }

    #[test]
    fn silence_is_finite() {
        let s = vec![0.0_f32; 48_000];
        let chans: [&[f32]; 1] = [&s];
        let mut out = AnalysisResult::default();
        LoudnessAnalyzer
            .analyze(
                AudioView {
                    sample_rate: SampleRate(48_000),
                    channels: &chans,
                },
                &mut out,
                &CancelFlag::new(),
                &mut |_| {},
            )
            .unwrap();
        let l = out.loudness.unwrap();
        assert_eq!(l.integrated_lufs, SILENCE_LUFS);
        assert_eq!(l.true_peak_dbtp, SILENCE_DBTP);
    }
}
