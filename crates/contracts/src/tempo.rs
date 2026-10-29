//! Tempo fader ranges, sync modes and jog constants.

/// Tempo fader range: how far a full fader throw changes playback speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TempoRange {
    /// +/- 6 %.
    Percent6,
    /// +/- 10 % (default).
    #[default]
    Percent10,
    /// +/- 16 %.
    Percent16,
    /// +/- 100 % (0x to 2x speed).
    Wide,
}

impl TempoRange {
    /// Ranges in the order the range button cycles through them.
    pub const CYCLE: [TempoRange; 4] = [
        TempoRange::Percent6,
        TempoRange::Percent10,
        TempoRange::Percent16,
        TempoRange::Wide,
    ];

    /// Maximum speed change as a fraction (`0.10` for +/- 10 %).
    pub const fn max_fraction(self) -> f64 {
        match self {
            TempoRange::Percent6 => 0.06,
            TempoRange::Percent10 => 0.10,
            TempoRange::Percent16 => 0.16,
            TempoRange::Wide => 1.0,
        }
    }

    /// Playback rate ratio for a fader `position` in `[-1, 1]` (`+1` = fastest).
    /// Out-of-range positions are clamped. RT-safe.
    pub fn ratio(self, position: f32) -> f64 {
        1.0 + f64::from(position.clamp(-1.0, 1.0)) * self.max_fraction()
    }

    /// Next range in [`TempoRange::CYCLE`], wrapping.
    pub fn next(self) -> TempoRange {
        let i = Self::CYCLE.iter().position(|r| *r == self).unwrap_or(0);
        Self::CYCLE[(i + 1) % Self::CYCLE.len()]
    }
}

/// How a deck follows the sync leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SyncMode {
    /// Free running.
    #[default]
    Off,
    /// Match the leader's tempo only.
    Tempo,
    /// Match tempo and beat phase.
    Beat,
}

/// Seconds of audio per platter revolution in vinyl (scratch) mode: a
/// 33 1/3 rpm record turns once every 1.8 s.
pub const VINYL_SECONDS_PER_REVOLUTION: f64 = 60.0 / (100.0 / 3.0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratios() {
        assert!((TempoRange::Percent10.ratio(1.0) - 1.1).abs() < 1e-9);
        assert!((TempoRange::Percent6.ratio(-1.0) - 0.94).abs() < 1e-9);
        assert!((TempoRange::Wide.ratio(-2.0) - 0.0).abs() < 1e-9);
        assert_eq!(TempoRange::Percent16.ratio(0.0), 1.0);
    }

    #[test]
    fn cycle_wraps() {
        assert_eq!(TempoRange::Wide.next(), TempoRange::Percent6);
        assert_eq!(TempoRange::Percent6.next(), TempoRange::Percent10);
    }

    #[test]
    fn vinyl_revolution() {
        assert!((VINYL_SECONDS_PER_REVOLUTION - 1.8).abs() < 1e-12);
    }
}
