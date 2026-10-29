//! Cue points and loops.
//!
//! Persisted and IPC cue positions are [`Seconds`] from the first decoded
//! frame (sample-rate independent). The engine receives them converted to
//! [`Frames`] at the engine rate via [`FrameRegion`].

use crate::HOT_CUES_PER_DECK;
use crate::ids::CueId;
use crate::units::{Frames, Seconds};

/// A display colour, 8-bit sRGB. Mapping to a controller's LED palette is the
/// MIDI layer's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CueColor {
    /// Red, 0..=255.
    pub r: u8,
    /// Green, 0..=255.
    pub g: u8,
    /// Blue, 0..=255.
    pub b: u8,
}

impl CueColor {
    /// Packs as `0xRRGGBB` (the `cues.color` column).
    pub const fn to_rgb24(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    /// Unpacks `0xRRGGBB`; the top byte is ignored.
    pub const fn from_rgb24(rgb: u32) -> Self {
        Self {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
        }
    }
}

/// Hot cue slot index, `0..HOT_CUES_PER_DECK` (pad 1 is index 0).
///
/// Invariant enforced by [`HotCueIndex::new`] and deserialization. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HotCueIndex(u8);

impl HotCueIndex {
    /// Returns the index, or `None` if `index >= HOT_CUES_PER_DECK`.
    pub const fn new(index: u8) -> Option<Self> {
        if (index as usize) < HOT_CUES_PER_DECK {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Zero-based index, guaranteed `< HOT_CUES_PER_DECK`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<u8> for HotCueIndex {
    type Error = crate::ids::OutOfRange;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        Self::new(index).ok_or(crate::ids::OutOfRange {
            value: index.into(),
            limit: HOT_CUES_PER_DECK as u32,
        })
    }
}

impl From<HotCueIndex> for u8 {
    fn from(index: HotCueIndex) -> Self {
        index.0
    }
}

/// A loop region in seconds. Invariant: `start < end`, both finite.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LoopRegion {
    /// Loop in point.
    pub start: Seconds,
    /// Loop out point (exclusive).
    pub end: Seconds,
}

impl LoopRegion {
    /// Returns true when `start < end` and both are finite.
    pub fn is_valid(&self) -> bool {
        self.start.0.is_finite() && self.end.0.is_finite() && self.start.0 < self.end.0
    }
}

/// An engine-side region in frames at the engine rate. RT-safe.
///
/// `end == None` is a point (cue); `Some(end)` is a loop with `start < end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FrameRegion {
    /// Start (cue point or loop in).
    pub start: Frames,
    /// Loop out (exclusive), or `None` for a point.
    pub end: Option<Frames>,
}

/// What a stored cue is (`cues.kind` column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CueKind {
    /// Hot cue on a pad. May be a hot loop when the cue has an `end`.
    Hot {
        /// Pad slot.
        index: HotCueIndex,
    },
    /// Memory cue, recalled with CUE/LOOP CALL. May be a loop when it has an `end`.
    Memory,
    /// Saved loop without a pad assignment.
    Loop,
}

/// A stored cue: hot cue, memory cue or saved loop.
///
/// Invariants: `start` is finite and `>= 0`; when `end` is `Some`, `start <
/// end`; `kind == Loop` requires `end`. At most one hot cue per
/// `(track, index)`. Not RT: owns a `String`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Cue {
    /// Row id, `None` before the cue is persisted.
    pub id: Option<CueId>,
    /// Hot, memory or loop.
    pub kind: CueKind,
    /// Cue point or loop in, seconds.
    pub start: Seconds,
    /// Loop out, seconds, if this cue is a loop.
    pub end: Option<Seconds>,
    /// Display colour, `None` for the theme default.
    pub color: Option<CueColor>,
    /// User label, may be empty.
    pub name: String,
}

impl Cue {
    /// Checks the documented invariants.
    pub fn is_valid(&self) -> bool {
        let start_ok = self.start.0.is_finite() && self.start.0 >= 0.0;
        let end_ok = match self.end {
            Some(end) => end.0.is_finite() && self.start.0 < end.0,
            None => !matches!(self.kind, CueKind::Loop),
        };
        start_ok && end_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_packing() {
        let c = CueColor {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        };
        assert_eq!(c.to_rgb24(), 0x12_34_56);
        assert_eq!(CueColor::from_rgb24(0xFF12_3456), c);
    }

    #[test]
    fn hot_cue_index_bounds() {
        assert!(HotCueIndex::new(7).is_some());
        assert!(HotCueIndex::new(8).is_none());
    }

    #[test]
    fn cue_validation() {
        let mut cue = Cue {
            id: None,
            kind: CueKind::Loop,
            start: Seconds(1.0),
            end: None,
            color: None,
            name: String::new(),
        };
        assert!(!cue.is_valid(), "loop without end");
        cue.end = Some(Seconds(2.0));
        assert!(cue.is_valid());
        cue.end = Some(Seconds(0.5));
        assert!(!cue.is_valid(), "end before start");
    }
}
