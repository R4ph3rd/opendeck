//! Offline analysis contract.
//!
//! An [`Analyzer`] fills part of an [`AnalysisResult`] from decoded audio.
//! The import pipeline runs several analyzers in sequence on a worker pool
//! (ADR 0003), each declaring which [`AnalysisFeatures`] it provides, so the
//! BPM/key implementation can be swapped (stratum-dsp or custom, ADR 0001)
//! without touching the pipeline. Never runs on the audio thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::beatgrid::BeatGrid;
use crate::key::MusicalKey;
use crate::peaks::PeakLevel;
use crate::units::{Bpm, SampleRate};

/// Borrowed decoded audio: planar `f32`, one slice per channel.
///
/// Invariants: 1 or 2 channels, all the same length, samples in `[-1, 1]`
/// nominal (overs allowed), `sample_rate > 0`.
#[derive(Debug, Clone, Copy)]
pub struct AudioView<'a> {
    /// Rate of the samples.
    pub sample_rate: SampleRate,
    /// Planar channels.
    pub channels: &'a [&'a [f32]],
}

impl AudioView<'_> {
    /// Frames per channel (0 if there are no channels).
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, |c| c.len())
    }

    /// Checks the documented invariants.
    pub fn is_valid(&self) -> bool {
        let n = self.frames();
        self.sample_rate.0 > 0
            && (1..=2).contains(&self.channels.len())
            && self.channels.iter().all(|c| c.len() == n)
    }
}

/// Which results an analyzer produces or a job requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AnalysisFeatures {
    /// BPM and beat grid.
    pub tempo: bool,
    /// Musical key.
    pub key: bool,
    /// EBU R128 loudness and true peak.
    pub loudness: bool,
    /// Waveform peaks.
    pub peaks: bool,
}

impl AnalysisFeatures {
    /// Every feature.
    pub const ALL: AnalysisFeatures = AnalysisFeatures {
        tempo: true,
        key: true,
        loudness: true,
        peaks: true,
    };
}

/// Integrated loudness and true peak (EBU R128 / ITU-R BS.1770).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Loudness {
    /// Integrated loudness, LUFS.
    pub integrated_lufs: f64,
    /// True peak, dBTP.
    pub true_peak_dbtp: f64,
}

/// Accumulated analysis output. Each analyzer fills the fields it provides.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnalysisResult {
    /// Detected tempo.
    pub bpm: Option<Bpm>,
    /// Detected beat grid.
    pub beat_grid: Option<BeatGrid>,
    /// Detected key.
    pub key: Option<MusicalKey>,
    /// Loudness.
    pub loudness: Option<Loudness>,
    /// Peak levels, one per zoom level, finest first.
    pub peaks: Vec<PeakLevel>,
}

/// Cooperative cancellation flag shared between a job and its controller.
/// Cheap to clone; checking it is a relaxed atomic load.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    /// New, not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// True once cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Error from an analyzer.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisError {
    /// The cancel flag was set; partial output must be discarded.
    Cancelled,
    /// Input violates [`AudioView`] invariants or is too short to analyze.
    InvalidInput(String),
    /// Analysis ran but failed.
    Failed(String),
}

impl core::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("analysis cancelled"),
            Self::InvalidInput(m) => write!(f, "invalid analysis input: {m}"),
            Self::Failed(m) => write!(f, "analysis failed: {m}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

/// One analysis stage. Runs on a worker thread; may allocate and take seconds.
///
/// Contract: deterministic (same input and version give the same output on
/// every platform); checks `cancel` at least every ~100 ms of work and
/// returns [`AnalysisError::Cancelled`] when set; calls `progress` with
/// values in `[0, 1]`, non-decreasing; writes only the fields named by
/// [`Analyzer::features`].
pub trait Analyzer: Send + Sync {
    /// Stable id, stored with results (`"stratum"`, `"peaks"`).
    fn id(&self) -> &str;

    /// Output version. Bump when results change for the same input, so the
    /// library can schedule re-analysis.
    fn version(&self) -> u32;

    /// What this analyzer fills in.
    fn features(&self) -> AnalysisFeatures;

    /// Analyzes `audio` and writes into `out`.
    fn analyze(
        &self,
        audio: AudioView<'_>,
        out: &mut AnalysisResult,
        cancel: &CancelFlag,
        progress: &mut dyn FnMut(f32),
    ) -> Result<(), AnalysisError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Silence;

    impl Analyzer for Silence {
        fn id(&self) -> &str {
            "silence"
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
            if !audio.is_valid() {
                return Err(AnalysisError::InvalidInput("bad view".into()));
            }
            if cancel.is_cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            progress(1.0);
            out.loudness = Some(Loudness {
                integrated_lufs: -70.0,
                true_peak_dbtp: -120.0,
            });
            Ok(())
        }
    }

    #[test]
    fn analyzer_fills_result_and_honours_cancel() {
        let samples = [0.0_f32; 64];
        let channels: [&[f32]; 2] = [&samples, &samples];
        let view = AudioView {
            sample_rate: SampleRate(44_100),
            channels: &channels,
        };
        assert_eq!(view.frames(), 64);

        let mut out = AnalysisResult::default();
        let cancel = CancelFlag::new();
        let mut last = 0.0;
        Silence
            .analyze(view, &mut out, &cancel, &mut |p| last = p)
            .expect("analyzes");
        assert_eq!(last, 1.0);
        assert!(out.loudness.is_some());

        cancel.clone().cancel();
        assert_eq!(
            Silence.analyze(view, &mut out, &cancel, &mut |_| {}),
            Err(AnalysisError::Cancelled)
        );
    }
}
