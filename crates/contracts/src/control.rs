//! Controller-agnostic control actions and LED feedback targets.
//!
//! The MIDI layer turns raw messages into [`ControlEvent`]s using a JSON
//! mapping (ADR 0007): the mapping names a [`ControlAction`] (what is
//! controlled) and decodes a [`ControlValue`] (how much). app-core turns
//! events into `EngineCommand`s and state changes, applying pad modes, shift
//! state and curves. Nothing here knows about MIDI bytes.
//!
//! For LEDs the direction is reversed: app-core publishes
//! `(FeedbackTarget, FeedbackValue)` pairs when state changes; the mapping
//! says which MIDI message lights which target.
//!
//! Serialized with `"action"` / `"target"` tags in snake_case, which is the
//! exact spelling used in mapping files.

use crate::cue::HotCueIndex;
use crate::ids::{DeckId, FxUnit, SamplerSlotId};
use crate::mixer::EqBand;
use crate::pad::PadMode;
use crate::units::Beats;

/// A decoded control value.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ControlValue {
    /// Button edge: `true` on press, `false` on release.
    Button(bool),
    /// Absolute position in `[0, 1]` (knobs, faders; 7- or 14-bit source).
    Absolute(f32),
    /// Relative movement in mapping-defined units (encoder ticks scaled by
    /// the mapping; jog wheels are scaled to revolutions). Signed, positive =
    /// clockwise / up.
    Relative(f32),
}

/// What a control does, independent of the hardware. Each variant lists the
/// [`ControlValue`] kind it expects; other kinds are ignored by app-core.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "action", rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ControlAction {
    // ----- Deck transport (Button) -----
    /// PLAY/PAUSE. Button.
    PlayPause {
        /// Deck.
        deck: DeckId,
    },
    /// CUE. Button (press and release both matter).
    Cue {
        /// Deck.
        deck: DeckId,
    },
    /// Return to the track start (SHIFT + CUE on many controllers). Button.
    ReturnToStart {
        /// Deck.
        deck: DeckId,
    },
    /// BEAT SYNC toggle. Button.
    Sync {
        /// Deck.
        deck: DeckId,
    },
    /// Make this deck the sync leader (see `EngineCommand::SetLeader`). Button.
    SyncLeader {
        /// Deck.
        deck: DeckId,
    },
    /// Load the selected browser track into this deck. Button.
    Load {
        /// Deck.
        deck: DeckId,
    },
    // ----- Tempo (Absolute / Button) -----
    /// Tempo fader. Absolute, 0 = slowest end, 1 = fastest end.
    TempoFader {
        /// Deck.
        deck: DeckId,
    },
    /// Cycle the tempo range. Button.
    TempoRangeNext {
        /// Deck.
        deck: DeckId,
    },
    /// Toggle key lock. Button.
    KeyLock {
        /// Deck.
        deck: DeckId,
    },
    /// Pitch bend while held, `direction` +1 faster or -1 slower. Button.
    PitchBend {
        /// Deck.
        deck: DeckId,
        /// +1 or -1.
        direction: i8,
    },
    // ----- Jog (Button / Relative) -----
    /// Platter touch. Button.
    JogTouch {
        /// Deck.
        deck: DeckId,
    },
    /// Platter top rotation: scratch in vinyl mode, bend otherwise. Relative, revolutions.
    JogTop {
        /// Deck.
        deck: DeckId,
    },
    /// Jog wheel side rotation: pitch bend. Relative, revolutions.
    JogSide {
        /// Deck.
        deck: DeckId,
    },
    /// SHIFT + jog: fast search. Relative, revolutions.
    JogSearch {
        /// Deck.
        deck: DeckId,
    },
    /// Toggle vinyl mode. Button.
    VinylMode {
        /// Deck.
        deck: DeckId,
    },
    // ----- Loops and jumps (Button) -----
    /// Loop in. Button.
    LoopIn {
        /// Deck.
        deck: DeckId,
    },
    /// Loop out. Button.
    LoopOut {
        /// Deck.
        deck: DeckId,
    },
    /// Exit or re-enter the loop. Button.
    LoopToggle {
        /// Deck.
        deck: DeckId,
    },
    /// Auto beat loop of `length`. Button.
    AutoLoop {
        /// Deck.
        deck: DeckId,
        /// Loop length.
        length: Beats,
    },
    /// Halve the active loop. Button.
    LoopHalve {
        /// Deck.
        deck: DeckId,
    },
    /// Double the active loop. Button.
    LoopDouble {
        /// Deck.
        deck: DeckId,
    },
    /// Beat jump by `beats` (signed). Button.
    BeatJump {
        /// Deck.
        deck: DeckId,
        /// Beats.
        beats: Beats,
    },
    /// Previous memory cue or loop (CUE/LOOP CALL left). Button.
    CueLoopCallPrev {
        /// Deck.
        deck: DeckId,
    },
    /// Next memory cue or loop (CUE/LOOP CALL right). Button.
    CueLoopCallNext {
        /// Deck.
        deck: DeckId,
    },
    /// Add a memory cue at the playhead. Button.
    MemoryCueAdd {
        /// Deck.
        deck: DeckId,
    },
    /// Toggle slip mode. Button.
    Slip {
        /// Deck.
        deck: DeckId,
    },
    /// Toggle quantize. Button.
    Quantize {
        /// Deck.
        deck: DeckId,
    },
    // ----- Pads (Button) -----
    /// Select a pad mode. Button.
    PadModeSelect {
        /// Deck.
        deck: DeckId,
        /// Mode.
        mode: PadMode,
    },
    /// Performance pad `index` in `mode`. app-core ignores the press if
    /// `mode` is not the deck's active mode. Button.
    Pad {
        /// Deck.
        deck: DeckId,
        /// Mode the pad message belongs to.
        mode: PadMode,
        /// Pad, `0..PADS_PER_DECK`.
        index: u8,
        /// SHIFT layer of the pad (second function, e.g. delete hot cue).
        shifted: bool,
    },
    /// Trigger or set a hot cue directly (keyboard shortcut, UI). Button.
    HotCue {
        /// Deck.
        deck: DeckId,
        /// Slot.
        index: HotCueIndex,
    },
    /// Delete a hot cue. Button.
    HotCueDelete {
        /// Deck.
        deck: DeckId,
        /// Slot.
        index: HotCueIndex,
    },
    // ----- Mixer -----
    /// Trim knob. Absolute.
    Trim {
        /// Channel.
        deck: DeckId,
    },
    /// EQ knob. Absolute, 0.5 = unity.
    Eq {
        /// Channel.
        deck: DeckId,
        /// Band.
        band: EqBand,
    },
    /// Filter knob. Absolute, 0.5 = off.
    Filter {
        /// Channel.
        deck: DeckId,
    },
    /// Channel fader. Absolute.
    ChannelFader {
        /// Channel.
        deck: DeckId,
    },
    /// Fader start event from the hardware (fader left or reached the bottom). Button.
    ChannelFaderStart {
        /// Channel.
        deck: DeckId,
    },
    /// Headphone cue (PFL) toggle. Button.
    Pfl {
        /// Channel.
        deck: DeckId,
    },
    /// Crossfader. Absolute, 0 = full A.
    Crossfader,
    /// Master level. Absolute.
    MasterLevel,
    /// Master to headphones (MASTER CUE) toggle. Button.
    MasterCue,
    /// Headphone level. Absolute.
    HeadphoneLevel,
    /// Headphone cue/master mix. Absolute, 0 = cue.
    HeadphoneMix,
    // ----- Effects -----
    /// Toggle an effect unit. Button.
    FxToggle {
        /// Unit.
        unit: FxUnit,
    },
    /// Effect level/depth (wet/dry or main parameter). Absolute.
    FxLevel {
        /// Unit.
        unit: FxUnit,
    },
    /// Next (+1) or previous (-1) effect type in the unit. Button.
    FxSelect {
        /// Unit.
        unit: FxUnit,
        /// +1 or -1.
        direction: i8,
    },
    /// Next (+1) or previous (-1) beat division. Button.
    FxBeat {
        /// Unit.
        unit: FxUnit,
        /// +1 or -1.
        direction: i8,
    },
    /// Route the beat effect unit to `target` (FX CH SELECT). Button.
    FxAssign {
        /// Unit being routed.
        unit: FxUnit,
        /// Target channel, or `None` for master.
        target: Option<DeckId>,
    },
    // ----- Sampler -----
    /// Sampler slot pad. Button.
    Sampler {
        /// Slot.
        slot: SamplerSlotId,
    },
    // ----- Browser -----
    /// Browser scroll. Relative, list rows.
    BrowseScroll,
    /// Browser encoder press (open folder / focus). Button.
    BrowsePress,
    /// Browser back (SHIFT + press on many controllers). Button.
    BrowseBack,
}

/// A decoded control input. `timestamp_us` is the MIDI driver timestamp in
/// microseconds (monotonic, arbitrary origin) for jog smoothing.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ControlEvent {
    /// What.
    pub action: ControlAction,
    /// How much.
    pub value: ControlValue,
    /// Driver timestamp, microseconds.
    pub timestamp_us: u64,
}

/// An application state that can drive a controller output (LED, meter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "target", rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FeedbackTarget {
    /// Deck is playing (blinks when paused with a track loaded, CDJ style).
    Playing {
        /// Deck.
        deck: DeckId,
    },
    /// CUE indicator.
    CueIndicator {
        /// Deck.
        deck: DeckId,
    },
    /// Sync on.
    SyncOn {
        /// Deck.
        deck: DeckId,
    },
    /// Loop active.
    LoopActive {
        /// Deck.
        deck: DeckId,
    },
    /// Headphone cue on.
    PflOn {
        /// Channel.
        deck: DeckId,
    },
    /// Pad mode button lit.
    PadModeActive {
        /// Deck.
        deck: DeckId,
        /// Mode.
        mode: PadMode,
    },
    /// A performance pad's light in a given mode (hot cue set, loop size active, ...).
    Pad {
        /// Deck.
        deck: DeckId,
        /// Mode.
        mode: PadMode,
        /// Pad index.
        index: u8,
    },
    /// Effect unit on.
    FxOn {
        /// Unit.
        unit: FxUnit,
    },
    /// Channel level meter.
    ChannelLevel {
        /// Channel.
        deck: DeckId,
    },
    /// A track is loaded.
    TrackLoaded {
        /// Deck.
        deck: DeckId,
    },
    /// Vinyl mode setting (the FLX4 receives it as a MIDI-OUT message).
    VinylMode {
        /// Deck.
        deck: DeckId,
    },
}

/// Value for a feedback target.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FeedbackValue {
    /// Light off.
    Off,
    /// Light on.
    On,
    /// Blinking (the mapping decides how: device blink message or timed toggles).
    Blink,
    /// Level in `[0, 1]` (meters).
    Level(f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<ControlAction>();
        assert_copy::<ControlEvent>();
        assert_copy::<FeedbackTarget>();
    }

    #[cfg(feature = "serde")]
    #[test]
    fn mapping_file_spelling() {
        let deck = DeckId::new(0).expect("deck 1");
        let json = serde_json::to_string(&ControlAction::PlayPause { deck }).expect("serializes");
        assert_eq!(json, r#"{"action":"play_pause","deck":0}"#);

        let eq: ControlAction =
            serde_json::from_str(r#"{"action":"eq","deck":1,"band":"high"}"#).expect("parses");
        assert_eq!(
            eq,
            ControlAction::Eq {
                deck: DeckId::new(1).expect("deck 2"),
                band: EqBand::High
            }
        );
        assert!(
            serde_json::from_str::<ControlAction>(r#"{"action":"play_pause","deck":7}"#).is_err(),
            "deck out of range must be rejected"
        );

        let value = serde_json::to_string(&ControlValue::Absolute(0.5)).expect("serializes");
        assert_eq!(value, r#"{"kind":"absolute","value":0.5}"#);
    }
}
