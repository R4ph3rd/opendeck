//! OpenDeck offline analysis: BPM and beat grid, musical key, waveform peaks
//! and loudness.
//!
//! Analysis is offline and deterministic: the same input gives the same output
//! on every platform. Implementations sit behind the
//! `opendeck_contracts::analysis::Analyzer` trait so `stratum-dsp` and a custom
//! detector can be swapped locally (ADR 0001). See `CONTEXT/analysis.md`.

/// Version of this crate's analysis output. Bump it when results change for
/// the same input, so the library can schedule re-analysis.
pub const ANALYSIS_VERSION: u32 = 0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_starts_at_zero() {
        assert_eq!(ANALYSIS_VERSION, 0);
    }
}
