//! OpenDeck offline analysis: BPM and beat grid, musical key, waveform peaks
//! and loudness.
//!
//! Analysis is offline and deterministic: the same input gives the same output
//! on every platform. Each stage is an
//! [`Analyzer`](opendeck_contracts::analysis::Analyzer) with a stable id and
//! version. `stratum-dsp` was evaluated and rejected (ADR 0001, see
//! `CONTEXT/analysis.md` for the numbers), so tempo and key are implemented
//! here. See `CONTEXT/analysis.md`.
//!
//! | Analyzer | id | Fills |
//! |---|---|---|
//! | [`PeaksAnalyzer`] | `peaks` | `peaks` |
//! | [`LoudnessAnalyzer`] | `loudness` | `loudness` |
//! | [`TempoAnalyzer`] | `tempo` | `bpm`, `beat_grid` |
//! | [`KeyAnalyzer`] | `key` | `key` |

mod dsp;
mod key;
mod loudness;
mod peaks;
mod tempo;

use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};

pub use key::KeyAnalyzer;
pub use loudness::LoudnessAnalyzer;
pub use peaks::{OVERVIEW_POINTS, PEAK_LEVEL_FRAMES, PeaksAnalyzer};
pub use tempo::{TempoAnalyzer, TempoConfig};

/// Version of this crate's analysis output. Bump it when results change for
/// the same input, so the library can schedule re-analysis.
pub const ANALYSIS_VERSION: u32 = 1;

/// The default analyzer set, in the order the pipeline should run them.
pub fn default_analyzers() -> Vec<Box<dyn Analyzer>> {
    vec![
        Box::new(PeaksAnalyzer),
        Box::new(LoudnessAnalyzer),
        Box::new(TempoAnalyzer::default()),
        Box::new(KeyAnalyzer),
    ]
}

/// Runs every default analyzer that `features` asks for and returns the
/// merged result. `progress` is monotonic in `[0, 1]` across all stages.
pub fn analyze_all(
    audio: AudioView<'_>,
    features: AnalysisFeatures,
    cancel: &CancelFlag,
    progress: &mut dyn FnMut(f32),
) -> Result<AnalysisResult, AnalysisError> {
    let stages: Vec<_> = default_analyzers()
        .into_iter()
        .filter(|a| {
            let f = a.features();
            (f.peaks && features.peaks)
                || (f.loudness && features.loudness)
                || (f.tempo && features.tempo)
                || (f.key && features.key)
        })
        .collect();
    let mut out = AnalysisResult::default();
    let n = stages.len().max(1) as f32;
    for (i, stage) in stages.iter().enumerate() {
        let base = i as f32 / n;
        stage.analyze(audio, &mut out, cancel, &mut |p| {
            progress(base + p.clamp(0.0, 1.0) / n);
        })?;
    }
    progress(1.0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_stable() {
        let ids: Vec<String> = default_analyzers()
            .iter()
            .map(|a| a.id().to_owned())
            .collect();
        assert_eq!(ids, ["peaks", "loudness", "tempo", "key"]);
    }
}
