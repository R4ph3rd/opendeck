//! Beat grid: where beats and bars fall in a track.
//!
//! Version 1 is a constant-tempo grid: an anchor (a downbeat) plus a BPM.
//! Variable-tempo tracks will get a separate tempo-map type with a list of
//! segments; it will be added next to [`BeatGrid`], stored with blob format
//! version 2, and the engine will keep receiving the segment that covers the
//! playhead. Nothing in v1 needs to change for that.

use crate::units::{Beats, Bpm, Seconds};

/// Current beat grid blob format version (`analysis.beat_grid` column).
pub const BEAT_GRID_FORMAT_VERSION: u8 = 1;

/// A constant-tempo beat grid. RT-safe.
///
/// Invariants (checked by [`BeatGrid::new`]): `bpm` is finite and `> 0`,
/// `anchor` is finite, `beats_per_bar >= 1`. `anchor` is the time of a
/// downbeat (beat 1 of a bar) in seconds from the first decoded frame; it may
/// be negative when the first downbeat precedes the audio. Beat `n` falls at
/// `anchor + n * 60 / bpm`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(try_from = "BeatGridRaw", into = "BeatGridRaw")
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BeatGrid {
    anchor: Seconds,
    bpm: Bpm,
    beats_per_bar: u8,
}

/// Unvalidated wire form of [`BeatGrid`]; deserialization validates it.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BeatGridRaw {
    /// Time of beat 0 (a downbeat), seconds.
    pub anchor: Seconds,
    /// Tempo at original speed.
    pub bpm: Bpm,
    /// Beats per bar.
    pub beats_per_bar: u8,
}

impl TryFrom<BeatGridRaw> for BeatGrid {
    type Error = BeatGridError;

    fn try_from(raw: BeatGridRaw) -> Result<Self, Self::Error> {
        Self::new(raw.anchor, raw.bpm, raw.beats_per_bar)
    }
}

impl From<BeatGrid> for BeatGridRaw {
    fn from(grid: BeatGrid) -> Self {
        Self {
            anchor: grid.anchor,
            bpm: grid.bpm,
            beats_per_bar: grid.beats_per_bar,
        }
    }
}

/// Error for an invalid grid or an unreadable grid blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatGridError {
    /// BPM not finite and positive, anchor not finite, or zero beats per bar.
    Invalid,
    /// Blob too short or with an unknown format version.
    BadBlob,
}

impl core::fmt::Display for BeatGridError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Invalid => f.write_str("invalid beat grid"),
            Self::BadBlob => f.write_str("unreadable beat grid blob"),
        }
    }
}

impl std::error::Error for BeatGridError {}

/// Size in bytes of a version 1 grid blob.
const BLOB_V1_LEN: usize = 1 + 8 + 8 + 1;

impl BeatGrid {
    /// Builds a grid, validating the invariants.
    pub fn new(anchor: Seconds, bpm: Bpm, beats_per_bar: u8) -> Result<Self, BeatGridError> {
        if bpm.is_valid() && anchor.0.is_finite() && beats_per_bar >= 1 {
            Ok(Self {
                anchor,
                bpm,
                beats_per_bar,
            })
        } else {
            Err(BeatGridError::Invalid)
        }
    }

    /// Time of beat 0 (a downbeat).
    pub fn anchor(&self) -> Seconds {
        self.anchor
    }

    /// Tempo at original speed.
    pub fn bpm(&self) -> Bpm {
        self.bpm
    }

    /// Beats per bar (4 for 4/4).
    pub fn beats_per_bar(&self) -> u8 {
        self.beats_per_bar
    }

    /// Fractional beat index at time `t` (negative before the anchor).
    pub fn beat_at(&self, t: Seconds) -> Beats {
        Beats((t.0 - self.anchor.0) * self.bpm.0 / 60.0)
    }

    /// Time of fractional beat index `beat`.
    pub fn time_of(&self, beat: Beats) -> Seconds {
        Seconds(self.anchor.0 + beat.0 * 60.0 / self.bpm.0)
    }

    /// Position inside the current beat at `t`, in `[0, 1)`.
    pub fn beat_phase(&self, t: Seconds) -> f64 {
        self.beat_at(t).0.rem_euclid(1.0)
    }

    /// Zero-based beat within the bar at `t`, `0..beats_per_bar`.
    pub fn beat_in_bar(&self, t: Seconds) -> u8 {
        let beat = self.beat_at(t).0.floor();
        beat.rem_euclid(f64::from(self.beats_per_bar)) as u8
    }

    /// Time of the beat nearest to `t` (used by quantize).
    pub fn nearest_beat(&self, t: Seconds) -> Seconds {
        self.time_of(Beats(self.beat_at(t).0.round()))
    }

    /// Encodes as a version 1 blob: `[version u8][anchor f64 LE][bpm f64 LE][beats_per_bar u8]`.
    pub fn to_blob(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BLOB_V1_LEN);
        out.push(BEAT_GRID_FORMAT_VERSION);
        out.extend_from_slice(&self.anchor.0.to_le_bytes());
        out.extend_from_slice(&self.bpm.0.to_le_bytes());
        out.push(self.beats_per_bar);
        out
    }

    /// Decodes a blob written by [`BeatGrid::to_blob`].
    pub fn from_blob(blob: &[u8]) -> Result<Self, BeatGridError> {
        if blob.len() != BLOB_V1_LEN || blob[0] != BEAT_GRID_FORMAT_VERSION {
            return Err(BeatGridError::BadBlob);
        }
        let f64_at = |at: usize| {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&blob[at..at + 8]);
            f64::from_le_bytes(bytes)
        };
        let anchor = Seconds(f64_at(1));
        let bpm = Bpm(f64_at(9));
        Self::new(anchor, bpm, blob[17])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> BeatGrid {
        BeatGrid::new(Seconds(0.25), Bpm(120.0), 4).expect("valid grid")
    }

    #[test]
    fn beat_math() {
        let g = grid();
        assert_eq!(g.beat_at(Seconds(0.25)), Beats(0.0));
        assert_eq!(g.beat_at(Seconds(1.25)), Beats(2.0));
        assert_eq!(g.time_of(Beats(4.0)), Seconds(2.25));
        assert!((g.beat_phase(Seconds(0.5)) - 0.5).abs() < 1e-12);
        assert_eq!(g.beat_in_bar(Seconds(2.3)), 0);
        assert_eq!(g.beat_in_bar(Seconds(0.0)), 3);
        assert_eq!(g.nearest_beat(Seconds(0.7)), Seconds(0.75));
    }

    #[test]
    fn blob_round_trip() {
        let g = grid();
        let blob = g.to_blob();
        assert_eq!(blob.len(), BLOB_V1_LEN);
        assert_eq!(BeatGrid::from_blob(&blob), Ok(g));
        assert_eq!(BeatGrid::from_blob(&blob[..5]), Err(BeatGridError::BadBlob));
    }

    #[test]
    fn rejects_invalid() {
        assert!(BeatGrid::new(Seconds(0.0), Bpm(0.0), 4).is_err());
        assert!(BeatGrid::new(Seconds(f64::NAN), Bpm(120.0), 4).is_err());
        assert!(BeatGrid::new(Seconds(0.0), Bpm(120.0), 0).is_err());
    }
}
