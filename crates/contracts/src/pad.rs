//! Performance pad modes.
//!
//! Source: Pioneer DJ "DDJ-FLX4 List of MIDI messages" E1, section 5
//! (PERFORMANCE). Buttons 5-1 to 5-4 select HOT CUE, PAD FX 1, BEAT JUMP and
//! SAMPLER; each also sends a distinct note with SHIFT held. Section 5-5 lists
//! eight pad layers per deck in this order: HOT CUE, PAD FX 1, BEAT JUMP,
//! SAMPLER, KEYBOARD, PAD FX 2, BEAT LOOP, KEY SHIFT (pad notes 0x00, 0x10,
//! ..., 0x70).
//!
//! The pad mode is application state owned by app-core, per deck. MIDI mode
//! buttons and UI clicks both change it; the middle UI panel follows it.

/// One of the eight pad modes. RT-safe (plain enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PadMode {
    /// Pads trigger or set hot cues 1-8 (button 5-1).
    #[default]
    HotCue,
    /// Pads trigger pad effects, bank 1 (button 5-2).
    PadFx1,
    /// Pads jump by fixed beat counts (button 5-3).
    BeatJump,
    /// Pads trigger sampler slots (button 5-4).
    Sampler,
    /// Pads play the hot cue chromatically (SHIFT + 5-1).
    Keyboard,
    /// Pads trigger pad effects, bank 2 (SHIFT + 5-2).
    PadFx2,
    /// Pads set auto beat loops (SHIFT + 5-3).
    BeatLoop,
    /// Pads shift the key by semitones (SHIFT + 5-4).
    KeyShift,
}

impl PadMode {
    /// All modes in message-list order (section 5-5; index = pad note >> 4).
    pub const ALL: [PadMode; 8] = [
        PadMode::HotCue,
        PadMode::PadFx1,
        PadMode::BeatJump,
        PadMode::Sampler,
        PadMode::Keyboard,
        PadMode::PadFx2,
        PadMode::BeatLoop,
        PadMode::KeyShift,
    ];

    /// Position in [`PadMode::ALL`], `0..8`.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// True for the four modes reached with SHIFT + a mode button.
    pub const fn is_shift_layer(self) -> bool {
        self.index() >= 4
    }

    /// The mode on the same button with the other SHIFT state.
    ///
    /// TODO(verify): the message list shows each mode button sending a second
    /// note with SHIFT but does not name the resulting mode. The pairing
    /// (mode `n` with mode `n + 4`) is inferred from the pad note layout and
    /// must be confirmed against the DDJ-FLX4 operating instructions.
    pub const fn shift_partner(self) -> PadMode {
        PadMode::ALL[(self.index() + 4) % 8]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_modes_in_order() {
        for (i, mode) in PadMode::ALL.iter().enumerate() {
            assert_eq!(mode.index(), i);
        }
    }

    #[test]
    fn shift_pairs() {
        assert_eq!(PadMode::HotCue.shift_partner(), PadMode::Keyboard);
        assert_eq!(PadMode::Sampler.shift_partner(), PadMode::KeyShift);
        assert_eq!(PadMode::BeatLoop.shift_partner(), PadMode::BeatJump);
        assert!(PadMode::PadFx2.is_shift_layer());
        assert!(!PadMode::PadFx1.is_shift_layer());
    }
}
