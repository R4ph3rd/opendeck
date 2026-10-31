//! The BPM range the tempo analyzer folds into, set from Settings, Library.
//!
//! [`BpmRange`] is shared between the library service (which stores it under
//! the `analysis.bpm_range` setting) and a tempo analyzer wrapper the import
//! workers run. A change applies to the next track analysed: new imports and
//! re-analysis. Tracks already analysed keep their BPM until re-analysed.

use std::sync::{Arc, Mutex, PoisonError};

use opendeck_analysis::{TempoAnalyzer, TempoConfig};
use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use serde::{Deserialize, Serialize};

use crate::types::CoreError;

/// Settings key of the stored range.
pub(crate) const BPM_RANGE_KEY: &str = "analysis.bpm_range";
/// Lowest BPM the range may start at.
pub const BPM_RANGE_MIN: f64 = 30.0;
/// Highest BPM the range may end at.
pub const BPM_RANGE_MAX: f64 = 300.0;

/// Lowest and highest tempo the analyzer reports, BPM. Octave errors fold
/// into the range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BpmRangeDto {
    /// Lowest BPM, at least 30.
    pub min_bpm: f64,
    /// Highest BPM, at most 300 and above `min_bpm`.
    pub max_bpm: f64,
}

impl Default for BpmRangeDto {
    fn default() -> Self {
        let d = TempoConfig::default();
        Self {
            min_bpm: d.min_bpm,
            max_bpm: d.max_bpm,
        }
    }
}

impl BpmRangeDto {
    /// Checks the bounds.
    pub fn validate(self) -> Result<Self, CoreError> {
        let ok = self.min_bpm.is_finite()
            && self.max_bpm.is_finite()
            && self.min_bpm >= BPM_RANGE_MIN
            && self.max_bpm <= BPM_RANGE_MAX
            && self.min_bpm < self.max_bpm;
        if ok {
            Ok(self)
        } else {
            Err(CoreError::Invalid(format!(
                "the BPM range must lie within {BPM_RANGE_MIN} to {BPM_RANGE_MAX}, lowest first"
            )))
        }
    }
}

/// Shared, changeable BPM range.
#[derive(Debug, Clone, Default)]
pub struct BpmRange(Arc<Mutex<BpmRangeDto>>);

impl BpmRange {
    /// The current range.
    pub fn get(&self) -> BpmRangeDto {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces the range. Validate first.
    pub(crate) fn set(&self, range: BpmRangeDto) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = range;
    }

    /// The tempo analyzer that reads this range on every track.
    pub fn analyzer(&self) -> Arc<dyn Analyzer> {
        Arc::new(RangedTempo {
            range: self.clone(),
            id: TempoAnalyzer::default().id().to_owned(),
        })
    }
}

/// [`TempoAnalyzer`] with the range read at the start of each track.
struct RangedTempo {
    range: BpmRange,
    id: String,
}

impl RangedTempo {
    fn current(&self) -> TempoAnalyzer {
        let r = self.range.get();
        TempoAnalyzer::new(TempoConfig {
            min_bpm: r.min_bpm,
            max_bpm: r.max_bpm,
        })
    }
}

impl Analyzer for RangedTempo {
    fn id(&self) -> &str {
        &self.id
    }

    fn version(&self) -> u32 {
        TempoAnalyzer::default().version()
    }

    fn features(&self) -> AnalysisFeatures {
        TempoAnalyzer::default().features()
    }

    fn analyze(
        &self,
        audio: AudioView<'_>,
        out: &mut AnalysisResult,
        cancel: &CancelFlag,
        progress: &mut dyn FnMut(f32),
    ) -> Result<(), AnalysisError> {
        self.current().analyze(audio, out, cancel, progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_bad_ranges() {
        let ok = BpmRangeDto {
            min_bpm: 60.0,
            max_bpm: 140.0,
        };
        assert!(ok.validate().is_ok());
        for (min_bpm, max_bpm) in [(20.0, 140.0), (60.0, 320.0), (140.0, 60.0), (f64::NAN, 1.0)] {
            assert!(BpmRangeDto { min_bpm, max_bpm }.validate().is_err());
        }
    }

    #[test]
    fn analyzer_follows_the_range() {
        let range = BpmRange::default();
        let analyzer = range.analyzer();
        assert_eq!(analyzer.id(), "tempo");
        assert!(analyzer.features().tempo);
        range.set(BpmRangeDto {
            min_bpm: 60.0,
            max_bpm: 140.0,
        });
        // A 150 BPM click track folds to 75 inside 60 to 140.
        let rate = 44_100u32;
        let seconds = 20usize;
        let mut ch = vec![0.0f32; rate as usize * seconds];
        let period = (f64::from(rate) * 60.0 / 150.0) as usize;
        for start in (0..ch.len()).step_by(period) {
            for (i, s) in ch.iter_mut().skip(start).take(400).enumerate() {
                *s = (1.0 - i as f32 / 400.0) * if i % 2 == 0 { 0.8 } else { -0.8 };
            }
        }
        let chans: [&[f32]; 2] = [&ch, &ch];
        let mut out = AnalysisResult::default();
        analyzer
            .analyze(
                AudioView {
                    channels: &chans,
                    sample_rate: opendeck_contracts::SampleRate(rate),
                },
                &mut out,
                &CancelFlag::new(),
                &mut |_| {},
            )
            .expect("analyzes");
        let bpm = out.bpm.expect("bpm").0;
        assert!((60.0..=140.0).contains(&bpm), "{bpm}");
    }
}
