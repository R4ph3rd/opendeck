//! Identifiers shared across crates.
//!
//! Database-backed ids ([`TrackId`], [`PlaylistId`], [`CueId`], [`JobId`]) wrap
//! the SQLite `INTEGER PRIMARY KEY` (`i64`, always positive). They are opaque:
//! never derive meaning from their numeric value. Engine-side ids
//! ([`DeckId`], [`FxSlotId`], [`SamplerSlotId`]) are small, bounded and
//! RT-safe: every constructor validates the bound, so the engine can index
//! fixed arrays with them without a bounds panic.

use crate::MAX_DECKS;

macro_rules! db_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        #[cfg_attr(feature = "serde", serde(transparent))]
        #[cfg_attr(feature = "specta", derive(specta::Type))]
        pub struct $name(pub i64);

        impl From<i64> for $name {
            fn from(raw: i64) -> Self {
                Self(raw)
            }
        }

        impl From<$name> for i64 {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

db_id!(
    /// A track row in the library (`tracks.id`).
    TrackId
);
db_id!(
    /// A playlist or playlist folder (`playlists.id`).
    PlaylistId
);
db_id!(
    /// A hot cue, memory cue or saved loop (`cues.id`).
    CueId
);
db_id!(
    /// An import or analysis job (`import_jobs.id`).
    JobId
);

/// A deck index, `0..MAX_DECKS`. Deck 1 in the UI is `DeckId(0)`.
///
/// Invariant: the inner index is always `< MAX_DECKS`. The field is private
/// and deserialization goes through [`DeckId::new`], so engine code may index
/// `[T; MAX_DECKS]` arrays with [`DeckId::index`] without a bounds check
/// failing. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeckId(u8);

impl DeckId {
    /// Every possible deck, in order.
    pub const ALL: [DeckId; MAX_DECKS] = [DeckId(0), DeckId(1), DeckId(2), DeckId(3)];

    /// Returns the deck with zero-based `index`, or `None` if `index >= MAX_DECKS`.
    pub const fn new(index: u8) -> Option<Self> {
        if (index as usize) < MAX_DECKS {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Zero-based index, guaranteed `< MAX_DECKS`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// One-based number as printed on the hardware and in the UI.
    pub const fn number(self) -> u8 {
        self.0 + 1
    }
}

/// Error for an out-of-range id during conversion or deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfRange {
    /// The rejected raw value.
    pub value: u32,
    /// Exclusive upper bound that was violated.
    pub limit: u32,
}

impl core::fmt::Display for OutOfRange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "value {} out of range 0..{}", self.value, self.limit)
    }
}

impl std::error::Error for OutOfRange {}

impl TryFrom<u8> for DeckId {
    type Error = OutOfRange;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        Self::new(index).ok_or(OutOfRange {
            value: index.into(),
            limit: MAX_DECKS as u32,
        })
    }
}

impl From<DeckId> for u8 {
    fn from(deck: DeckId) -> Self {
        deck.0
    }
}

/// Number of effect slots chained in series inside one [`FxUnit`].
///
/// Product decision for the MVP, not a hardware spec. Changing it is a
/// contract change because engine arrays are sized by it.
pub const FX_SLOTS_PER_UNIT: usize = 3;

/// Number of send/return effect buses.
pub const FX_SEND_BUSES: usize = 2;

/// Where an effect chain sits in the signal flow (ADR 0006). RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "index", rename_all = "snake_case")
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FxUnit {
    /// Insert chain on a deck channel, post-EQ, pre-fader.
    Deck(DeckId),
    /// Insert chain on the master bus, pre master level.
    Master,
    /// Send/return bus `0..FX_SEND_BUSES`; its return is summed into master.
    Send(u8),
}

/// One effect slot: a unit plus a position `0..FX_SLOTS_PER_UNIT` in its chain.
///
/// Invariant: `slot < FX_SLOTS_PER_UNIT` and, for [`FxUnit::Send`], the bus
/// index is `< FX_SEND_BUSES`. Build with [`FxSlotId::new`]. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "FxSlotRaw", into = "FxSlotRaw"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxSlotId {
    unit: FxUnit,
    slot: u8,
}

/// Unvalidated wire form of [`FxSlotId`]; deserialization validates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxSlotRaw {
    /// Chain the slot belongs to.
    pub unit: FxUnit,
    /// Position in the chain.
    pub slot: u8,
}

impl TryFrom<FxSlotRaw> for FxSlotId {
    type Error = OutOfRange;

    fn try_from(raw: FxSlotRaw) -> Result<Self, Self::Error> {
        Self::new(raw.unit, raw.slot).ok_or(OutOfRange {
            value: raw.slot.into(),
            limit: FX_SLOTS_PER_UNIT as u32,
        })
    }
}

impl From<FxSlotId> for FxSlotRaw {
    fn from(id: FxSlotId) -> Self {
        Self {
            unit: id.unit,
            slot: id.slot,
        }
    }
}

impl FxSlotId {
    /// Returns the slot, or `None` if `slot` or the send bus index is out of range.
    pub const fn new(unit: FxUnit, slot: u8) -> Option<Self> {
        let unit_ok = match unit {
            FxUnit::Send(bus) => (bus as usize) < FX_SEND_BUSES,
            FxUnit::Deck(_) | FxUnit::Master => true,
        };
        if unit_ok && (slot as usize) < FX_SLOTS_PER_UNIT {
            Some(Self { unit, slot })
        } else {
            None
        }
    }

    /// The chain this slot belongs to.
    pub const fn unit(self) -> FxUnit {
        self.unit
    }

    /// Position in the chain, `< FX_SLOTS_PER_UNIT`. Slot 0 processes first.
    pub const fn slot(self) -> usize {
        self.slot as usize
    }
}

/// Number of sampler slots.
///
/// The DDJ-FLX4 message list (section 5-5) shows 8 SAMPLER pads per deck on
/// MIDI channels 8 (deck 1) and 10 (deck 2), plus shifted variants on
/// channels 9 and 11. TODO(verify): whether deck 2's pads address the same 8
/// slots or 8 more. We assume 16 independent slots (8 per deck side) until
/// confirmed.
pub const SAMPLER_SLOTS: usize = 16;

/// A sampler slot, `0..SAMPLER_SLOTS`. RT-safe; constructor validates the bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "u8", into = "u8"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SamplerSlotId(u8);

impl SamplerSlotId {
    /// Returns the slot, or `None` if `index >= SAMPLER_SLOTS`.
    pub const fn new(index: u8) -> Option<Self> {
        if (index as usize) < SAMPLER_SLOTS {
            Some(Self(index))
        } else {
            None
        }
    }

    /// Zero-based index, guaranteed `< SAMPLER_SLOTS`.
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl TryFrom<u8> for SamplerSlotId {
    type Error = OutOfRange;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        Self::new(index).ok_or(OutOfRange {
            value: index.into(),
            limit: SAMPLER_SLOTS as u32,
        })
    }
}

impl From<SamplerSlotId> for u8 {
    fn from(slot: SamplerSlotId) -> Self {
        slot.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_id_bounds() {
        assert_eq!(DeckId::new(3).map(DeckId::number), Some(4));
        assert!(DeckId::new(4).is_none());
        assert!(DeckId::try_from(9).is_err());
        assert_eq!(DeckId::ALL.len(), MAX_DECKS);
    }

    #[test]
    fn fx_slot_bounds() {
        assert!(FxSlotId::new(FxUnit::Master, 0).is_some());
        assert!(FxSlotId::new(FxUnit::Master, FX_SLOTS_PER_UNIT as u8).is_none());
        assert!(FxSlotId::new(FxUnit::Send(FX_SEND_BUSES as u8), 0).is_none());
    }

    #[test]
    fn sampler_slot_bounds() {
        assert!(SamplerSlotId::new(15).is_some());
        assert!(SamplerSlotId::new(16).is_none());
    }
}
