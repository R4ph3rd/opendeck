//! Unit newtypes and conversions.
//!
//! Every type here is `Copy`, heap-free and RT-safe. Conversions are plain
//! arithmetic: they never allocate or panic. Inputs outside a documented range
//! give a saturated or non-finite result, never a panic; callers validate at
//! the edges (IPC, MIDI, file parsing).

/// A sample rate in Hz. Invariant: `> 0` wherever it is used as a divisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SampleRate(pub u32);

impl Default for SampleRate {
    /// 48 kHz, the engine's default device rate.
    fn default() -> Self {
        SampleRate(48_000)
    }
}

impl SampleRate {
    /// Sample rate as `f64`, for time arithmetic.
    pub const fn as_f64(self) -> f64 {
        self.0 as f64
    }
}

/// A count or position in sample frames (one frame = one sample per channel).
///
/// Always paired with a [`SampleRate`] stated by the context: engine frames are
/// at the device rate, file frames at the file rate. Positions are measured
/// from the first decoded frame of the track and are never negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Frames(pub u64);

impl Frames {
    /// Zero frames.
    pub const ZERO: Frames = Frames(0);

    /// Converts to seconds at `rate`.
    pub fn to_seconds(self, rate: SampleRate) -> Seconds {
        Seconds(self.0 as f64 / rate.as_f64())
    }
}

/// A duration or position in seconds, `f64`.
///
/// Persisted positions (cues, grids, loops) use seconds from the first
/// decoded frame so they survive a change of engine sample rate. May be
/// negative only where documented (beat grid anchors before the first frame).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Seconds(pub f64);

impl Seconds {
    /// Converts to frames at `rate`, rounding to the nearest frame.
    /// Negative and non-finite values saturate to 0.
    pub fn to_frames(self, rate: SampleRate) -> Frames {
        let frames = (self.0 * rate.as_f64()).round();
        // `as` saturates: NaN and negatives become 0, huge values u64::MAX.
        Frames(frames as u64)
    }
}

/// Tempo in beats per minute. Valid values are finite and `> 0`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Bpm(pub f64);

impl Bpm {
    /// Returns true when the tempo is finite and positive.
    pub fn is_valid(self) -> bool {
        self.0.is_finite() && self.0 > 0.0
    }

    /// Length of one beat.
    pub fn beat_length(self) -> Seconds {
        Seconds(60.0 / self.0)
    }

    /// Tempo after applying a playback rate ratio (`1.0` = original speed).
    pub fn scaled(self, ratio: f64) -> Bpm {
        Bpm(self.0 * ratio)
    }
}

/// A musical length or position in beats (quarter notes in 4/4), `f64`.
///
/// Signed: negative values mean backwards (beat jump) or before the grid
/// anchor. Loop and effect lengths are positive.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Beats(pub f64);

impl Beats {
    /// Loop, loop roll and beat jump sizes offered by the UI, in beats.
    /// Powers of two from 1/32 to 32.
    pub const STANDARD_SIZES: [Beats; 11] = [
        Beats(1.0 / 32.0),
        Beats(1.0 / 16.0),
        Beats(1.0 / 8.0),
        Beats(0.25),
        Beats(0.5),
        Beats(1.0),
        Beats(2.0),
        Beats(4.0),
        Beats(8.0),
        Beats(16.0),
        Beats(32.0),
    ];

    /// Duration of this many beats at `bpm`.
    pub fn to_seconds(self, bpm: Bpm) -> Seconds {
        Seconds(self.0 * 60.0 / bpm.0)
    }
}

/// Lowest level in dB that [`gain_to_db`] reports; quieter values clamp here.
pub const MIN_DB: f32 = -120.0;

/// Converts decibels to a linear amplitude factor. `0 dB` is `1.0`.
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear amplitude factor to decibels, clamped at [`MIN_DB`].
/// Zero and negative input give `MIN_DB`.
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        return MIN_DB;
    }
    (20.0 * gain.log10()).max(MIN_DB)
}

/// A linear amplitude factor (`1.0` = unity, `0.0` = silence). Valid values
/// are finite and `>= 0`. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Gain(pub f32);

impl Gain {
    /// Unity gain (0 dB).
    pub const UNITY: Gain = Gain(1.0);
    /// Silence.
    pub const SILENCE: Gain = Gain(0.0);

    /// Builds a gain from decibels.
    pub fn from_db(db: f32) -> Gain {
        Gain(db_to_gain(db))
    }

    /// Level in decibels, clamped at [`MIN_DB`].
    pub fn to_db(self) -> f32 {
        gain_to_db(self.0)
    }
}

impl Default for Gain {
    fn default() -> Self {
        Gain::UNITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_seconds_round_trip() {
        let rate = SampleRate(48_000);
        assert_eq!(Seconds(1.5).to_frames(rate), Frames(72_000));
        assert_eq!(Frames(24_000).to_seconds(rate), Seconds(0.5));
        assert_eq!(Seconds(-1.0).to_frames(rate), Frames(0));
        assert_eq!(Seconds(f64::NAN).to_frames(rate), Frames(0));
    }

    #[test]
    fn db_helpers() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_gain(-6.0) - 0.501_187).abs() < 1e-5);
        assert!((gain_to_db(0.5) + 6.0206).abs() < 1e-3);
        assert_eq!(gain_to_db(0.0), MIN_DB);
        assert!((Gain::from_db(6.0).to_db() - 6.0).abs() < 1e-4);
    }

    #[test]
    fn beats_and_bpm() {
        let bpm = Bpm(120.0);
        assert_eq!(bpm.beat_length(), Seconds(0.5));
        assert_eq!(Beats(4.0).to_seconds(bpm), Seconds(2.0));
        assert!(!Bpm(0.0).is_valid());
        assert!((bpm.scaled(1.08).0 - 129.6).abs() < 1e-9);
    }
}
