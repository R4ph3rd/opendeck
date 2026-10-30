//! IPC and persistence types for the FX control model (milestone 5).
//!
//! The model sits on top of the engine's FX units (ADR 0006): two FLX4-style
//! Beat FX units, a Sound Color FX per channel, 32 pad effects and two send
//! buses. [`FxStateDto`] is both what the UI sees and what is stored in the
//! library settings (`fx.state`).

use opendeck_contracts::MVP_DECKS;
use opendeck_contracts::fx::{EffectKey, FxRouting, ParamDescriptor, ParamId};
use opendeck_contracts::ids::{FX_SEND_BUSES, FX_SLOTS_PER_UNIT};
use serde::{Deserialize, Serialize};

/// Number of hardware-style Beat FX units.
pub const BEAT_FX_UNITS: usize = 2;
/// Effect slots of a Beat FX unit or a send unit (Multi mode runs three).
/// Deck chains keep their other `FX_SLOTS_PER_UNIT` slots for Sound Color FX,
/// pad FX and Release FX.
pub const FX_CHAIN_SLOTS: usize = 3;
/// Pad effects: 2 banks x 2 shift layers x 8 pads.
pub const PAD_FX_COUNT: usize = 32;
/// Pads per bank layer.
pub const PADS_PER_LAYER: usize = 8;

/// Channel a Beat FX unit is assigned to (BEAT FX CH SELECT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FxAssign {
    /// Not assigned (another unit took the channel).
    None,
    /// Channel 1 (deck A).
    Ch1,
    /// Channel 2 (deck B).
    Ch2,
    /// Both channels, mirrored (1&2).
    Both,
}

/// Multi or Single mode of a Beat FX unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FxMode {
    /// Up to three effects, one primary parameter each.
    Multi,
    /// One effect, every parameter editable.
    Single,
}

/// Tempo source of a Beat FX unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FxBpmSource {
    /// Track BPM.
    Auto,
    /// Tapped BPM.
    Tap,
}

/// Which family of engine units a reference points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FxTarget {
    /// A Beat FX unit (`0..BEAT_FX_UNITS`).
    Beat,
    /// A send/return bus (`0..FX_SEND_BUSES`).
    Send,
}

/// A Beat FX unit or a send bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxUnitRef {
    /// Family.
    pub target: FxTarget,
    /// Index inside the family.
    pub unit: u8,
}

/// One slot of a [`FxUnitRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxSlotRef {
    /// Owner.
    pub unit: FxUnitRef,
    /// Position `0..FX_CHAIN_SLOTS`.
    pub slot: u8,
}

/// One effect slot of a unit. `params` are normalized values in the effect's
/// descriptor order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxSlotDto {
    /// Effect, `None` for an empty slot.
    pub effect: Option<EffectKey>,
    /// Normalized parameters, descriptor order.
    pub params: Vec<f32>,
    /// Wet/dry `[0, 1]`.
    pub wet: f32,
    /// Beat division in beats.
    pub division: f64,
    /// Beat units: slot takes part when the unit is on. Send units: slot on.
    pub enabled: bool,
}

impl FxSlotDto {
    /// Empty slot.
    pub fn empty(enabled: bool) -> Self {
        Self {
            effect: None,
            params: Vec::new(),
            wet: 1.0,
            division: 1.0,
            enabled,
        }
    }
}

/// A Beat FX unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BeatFxUnitDto {
    /// Channel assignment.
    pub assign: FxAssign,
    /// Multi or Single.
    pub mode: FxMode,
    /// `FX_CHAIN_SLOTS` slots. Single mode uses slot 0 only.
    pub slots: Vec<FxSlotDto>,
    /// ON/OFF.
    pub on: bool,
    /// BPM Auto or Tap.
    pub bpm_source: FxBpmSource,
    /// Last tapped tempo.
    pub tap_bpm: Option<f64>,
    /// Effect played by Release FX.
    pub release_effect: Option<EffectKey>,
    /// How long the release effect runs, milliseconds.
    pub release_hold_ms: u32,
}

/// Sound Color FX of one channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CfxDto {
    /// CFX type (one of `FxStateDto::cfx_kinds`).
    pub kind: EffectKey,
    /// Knob position `[-1, 1]`, 0 is neutral.
    pub amount: f32,
}

/// A send/return bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SendUnitDto {
    /// `FX_CHAIN_SLOTS` slots.
    pub slots: Vec<FxSlotDto>,
    /// Send tapped after the channel fader (default) or before it.
    pub post_fader: bool,
}

/// What a pad does when held.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PadFxAssignDto {
    /// Effect, `None` for an empty pad.
    pub effect: Option<EffectKey>,
    /// Beat division in beats.
    pub division: f64,
    /// Release FX pad: press fires the release effect instead of holding it.
    pub release: bool,
}

/// Everything the UI shows of the FX model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxStateDto {
    /// `BEAT_FX_UNITS` Beat FX units.
    pub units: Vec<BeatFxUnitDto>,
    /// One CFX per channel (`MVP_DECKS`).
    pub cfx: Vec<CfxDto>,
    /// Effect types the CFX selector offers.
    pub cfx_kinds: Vec<EffectKey>,
    /// `FX_SEND_BUSES` send units.
    pub sends: Vec<SendUnitDto>,
    /// Send levels `[deck][bus]`, linear `[0, 1]`.
    pub send_levels: Vec<Vec<f32>>,
    /// `PAD_FX_COUNT` pad effects, index `bank * 16 + shift * 8 + pad`.
    pub pads: Vec<PadFxAssignDto>,
}

/// One effect for the UI: contract info plus catalog flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxEffectDto {
    /// Unique key.
    pub key: EffectKey,
    /// Display name.
    pub name: String,
    /// Intended routing.
    pub routing: FxRouting,
    /// Reads the beat division.
    pub beat_synced: bool,
    /// Output continues after input stops.
    pub has_tail: bool,
    /// Parameters in display order.
    pub params: Vec<ParamDescriptor>,
    /// Parameter the LEVEL/DEPTH control drives.
    pub primary: Option<ParamId>,
    /// Release-category effect.
    pub release: bool,
    /// Beat divisions the effect accepts, ascending; empty if not synced.
    pub divisions: Vec<f64>,
}

/// One effect of a stored chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxChainSlotDto {
    /// Effect.
    pub effect: EffectKey,
    /// Normalized parameters by id.
    pub params: Vec<(ParamId, f32)>,
    /// Wet/dry.
    pub wet: f32,
    /// Beat division.
    pub division: f64,
    /// Slot enabled.
    pub enabled: bool,
}

/// A stored chain: the effects of one unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxChainPresetDto {
    /// Unique name.
    pub name: String,
    /// At most `FX_CHAIN_SLOTS` slots.
    pub slots: Vec<FxChainSlotDto>,
}

/// Presets for the browser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxPresetsDto {
    /// Factory effect presets (read-only).
    pub factory: Vec<opendeck_contracts::fx::FxPreset>,
    /// User effect presets (`fx_presets` table).
    pub user: Vec<opendeck_contracts::fx::FxPreset>,
    /// User chain presets.
    pub chains: Vec<FxChainPresetDto>,
}

/// One change to the FX model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FxOp {
    /// Assigns a Beat FX unit to channel 1, 2 or 1&2.
    SetAssign {
        /// Unit.
        unit: u8,
        /// Assignment.
        assign: FxAssign,
    },
    /// Multi or Single mode.
    SetMode {
        /// Unit.
        unit: u8,
        /// Mode.
        mode: FxMode,
    },
    /// Chooses the effect of a slot (`None` empties it).
    SetEffect {
        /// Slot.
        slot: FxSlotRef,
        /// Effect.
        effect: Option<EffectKey>,
    },
    /// Sets one parameter, normalized.
    SetParam {
        /// Slot.
        slot: FxSlotRef,
        /// Parameter.
        param: ParamId,
        /// Normalized value.
        value: f32,
    },
    /// LEVEL/DEPTH: sets the effect's primary parameter, normalized.
    SetLevel {
        /// Slot.
        slot: FxSlotRef,
        /// Normalized value.
        value: f32,
    },
    /// Wet/dry.
    SetWet {
        /// Slot.
        slot: FxSlotRef,
        /// `[0, 1]`.
        wet: f32,
    },
    /// Beat division of one slot.
    SetDivision {
        /// Slot.
        slot: FxSlotRef,
        /// Beats.
        beats: f64,
    },
    /// BEAT left (`-1`) or right (`1`): steps every slot of the unit through
    /// its effect's division list.
    StepDivision {
        /// Unit.
        unit: FxUnitRef,
        /// Direction, sign only.
        direction: i8,
    },
    /// Slot takes part (Beat FX) or is on (send).
    SetSlotEnabled {
        /// Slot.
        slot: FxSlotRef,
        /// State.
        on: bool,
    },
    /// ON/OFF of a Beat FX unit.
    SetUnitOn {
        /// Unit.
        unit: u8,
        /// State.
        on: bool,
    },
    /// Release FX (SHIFT+ON/OFF).
    Release {
        /// Unit.
        unit: u8,
    },
    /// BPM Auto or Tap.
    SetBpmSource {
        /// Unit.
        unit: u8,
        /// Source.
        source: FxBpmSource,
    },
    /// One tap of the tap tempo; switches the unit to Tap.
    Tap {
        /// Unit.
        unit: u8,
    },
    /// Release effect and hold time of a unit.
    SetReleaseEffect {
        /// Unit.
        unit: u8,
        /// Effect.
        effect: Option<EffectKey>,
        /// Hold, milliseconds.
        hold_ms: u32,
    },
    /// CFX type of a channel.
    SetCfxKind {
        /// Channel deck.
        deck: opendeck_contracts::DeckId,
        /// Effect (must be in `cfx_kinds`).
        effect: EffectKey,
    },
    /// CFX knob.
    SetCfx {
        /// Channel deck.
        deck: opendeck_contracts::DeckId,
        /// `[-1, 1]`.
        amount: f32,
    },
    /// Send level of a channel to a bus.
    SetSendLevel {
        /// Channel deck.
        deck: opendeck_contracts::DeckId,
        /// Bus.
        bus: u8,
        /// Linear `[0, 1]`.
        level: f32,
    },
    /// Send tap point.
    SetSendTap {
        /// Bus.
        bus: u8,
        /// Post-fader when true.
        post_fader: bool,
    },
    /// Loads a stored effect preset into a slot.
    ApplyPreset {
        /// Slot.
        slot: FxSlotRef,
        /// Preset (effect must match or the slot switches to it).
        preset: opendeck_contracts::fx::FxPreset,
    },
    /// Loads a stored chain into a unit.
    ApplyChain {
        /// Unit.
        unit: FxUnitRef,
        /// Chain name.
        name: String,
    },
    /// Assigns a pad effect.
    SetPad {
        /// Index `0..PAD_FX_COUNT`.
        index: u8,
        /// Assignment.
        pad: PadFxAssignDto,
    },
}

const _: () = {
    assert!(MVP_DECKS == 2);
    assert!(FX_SEND_BUSES == 2);
    assert!(FX_CHAIN_SLOTS + 3 == FX_SLOTS_PER_UNIT);
};
