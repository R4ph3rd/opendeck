//! IPC types of the MIDI service (milestone 6, wave 6b).
//!
//! The midi crate has no specta derives, so the shell-facing shapes live here
//! and are converted from the midi types at the service boundary.

use opendeck_contracts::DeckId;
use opendeck_contracts::control::ControlAction;
use opendeck_contracts::ids::{PlaylistId, TrackId};
use opendeck_contracts::pad::PadMode;
use opendeck_midi::mapping::MidiInput;
use serde::{Deserialize, Serialize};

use crate::dto::TrackQueryDto;

/// A MIDI port the system reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiPortDto {
    /// Port name; used to select the device.
    pub name: String,
    /// An input port with this name exists.
    pub has_input: bool,
    /// An output port with this name exists (LED feedback).
    pub has_output: bool,
    /// The active mapping's device patterns match this name.
    pub matches_mapping: bool,
}

/// Controller connection state, for the top bar and the settings page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiStatusDto {
    /// Id of the active mapping.
    pub mapping_id: String,
    /// Display name of the active mapping.
    pub mapping_name: String,
    /// The MIDI worker is running (it looks for the device every second).
    pub enabled: bool,
    /// An input port is open.
    pub connected: bool,
    /// Open input port.
    pub input: Option<String>,
    /// Open output port.
    pub output: Option<String>,
    /// Last open or send error.
    pub last_error: Option<String>,
    /// Start the worker at launch.
    pub auto_connect: bool,
    /// Device chosen by hand; `None` follows the mapping's device patterns.
    pub device: Option<String>,
    /// Hardware fader start moves play and pause.
    pub fader_start: bool,
    /// Input messages dropped (channel full or SysEx).
    pub dropped_input: u64,
    /// `verify` markers left in the mapping.
    pub verify_count: u32,
}

/// What the user can change in the MIDI settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiSettingsDto {
    /// Start the worker at launch and connect when the device is present.
    pub auto_connect: bool,
    /// Device chosen by hand.
    pub device: Option<String>,
    /// Hardware fader start moves play and pause.
    pub fader_start: bool,
    /// User changes over the shipped mapping.
    pub overrides: Vec<MidiOverrideDto>,
}

impl Default for MidiSettingsDto {
    fn default() -> Self {
        Self {
            auto_connect: true,
            device: None,
            fader_start: false,
            overrides: Vec::new(),
        }
    }
}

/// A MIDI input, as the mapping files spell it. Channels are 1 to 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MidiBindingDto {
    /// A note.
    Note {
        /// Channel 1 to 16.
        channel: u8,
        /// Note number.
        number: u8,
    },
    /// A 7-bit control change.
    Cc {
        /// Channel 1 to 16.
        channel: u8,
        /// Controller number.
        number: u8,
    },
    /// A 14-bit control change pair.
    Cc14 {
        /// Channel 1 to 16.
        channel: u8,
        /// MSB controller number.
        msb: u8,
        /// LSB controller number.
        lsb: u8,
    },
}

impl From<MidiInput> for MidiBindingDto {
    fn from(input: MidiInput) -> Self {
        match input {
            MidiInput::Note { channel, number } => Self::Note { channel, number },
            MidiInput::Cc { channel, number } => Self::Cc { channel, number },
            MidiInput::Cc14 { channel, msb, lsb } => Self::Cc14 { channel, msb, lsb },
        }
    }
}

impl From<MidiBindingDto> for MidiInput {
    fn from(binding: MidiBindingDto) -> Self {
        match binding {
            MidiBindingDto::Note { channel, number } => Self::Note { channel, number },
            MidiBindingDto::Cc { channel, number } => Self::Cc { channel, number },
            MidiBindingDto::Cc14 { channel, msb, lsb } => Self::Cc14 { channel, msb, lsb },
        }
    }
}

/// One stored user change: control `control_id` listens to `binding`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiOverrideDto {
    /// Control id in the mapping, e.g. `deck1.play`.
    pub control_id: String,
    /// The input the control listens to now.
    pub binding: MidiBindingDto,
    /// `version` of the shipped mapping when the override was made; the UI
    /// flags an override whose base changed.
    pub base_version: u32,
}

/// One row of the mapping table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiMappingRowDto {
    /// Control id.
    pub id: String,
    /// Name from the controller's message list.
    pub label: Option<String>,
    /// Row or figure id in the source document.
    pub source_ref: Option<String>,
    /// Current MIDI input (with the user's override applied).
    pub binding: MidiBindingDto,
    /// What the control does.
    pub action: ControlAction,
    /// Layer: `base` or the modifier that must be held (`shift_deck1`).
    pub layer: String,
    /// `button`, `absolute` or `relative`.
    pub value_kind: String,
    /// The `verify` note, when the entry is unconfirmed.
    pub verify: Option<String>,
    /// The binding differs from the shipped mapping.
    pub overridden: bool,
}

/// Direction of a monitored message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum MidiMonitorDirection {
    /// From the controller.
    In,
    /// To the controller.
    Out,
}

/// What the mapping made of a monitored message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MidiMonitorLabelDto {
    /// Output message.
    Output,
    /// Matched a control.
    Control {
        /// Control id.
        id: String,
        /// Control name from the source document.
        label: Option<String>,
        /// The action it triggers.
        action: Option<ControlAction>,
        /// Unconfirmed value note.
        verify: Option<String>,
    },
    /// Matched a modifier (SHIFT).
    Modifier {
        /// Modifier id.
        id: String,
        /// Unconfirmed value note.
        verify: Option<String>,
    },
    /// Half of a 14-bit pair.
    Pending {
        /// Control id.
        id: String,
    },
    /// Listed in the source, deliberately not mapped.
    KnownUnmapped {
        /// Source row.
        source_ref: String,
        /// Why.
        reason: String,
    },
    /// Not in the mapping.
    Unmapped,
    /// Not a channel message the mapping covers.
    Ignored,
}

/// One line of the MIDI monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiMonitorEntryDto {
    /// Timestamp in microseconds on the worker clock.
    pub ts_us: u64,
    /// In or out.
    pub direction: MidiMonitorDirection,
    /// Raw bytes.
    pub bytes: Vec<u8>,
    /// Readable form, e.g. `Note On ch1 0x0B vel 127`.
    pub text: String,
    /// What the mapping made of it.
    pub label: MidiMonitorLabelDto,
}

/// Result of a MIDI learn gesture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MidiLearnResultDto {
    /// Control the gesture was learned for.
    pub control_id: String,
    /// The captured input, when one was captured and accepted.
    pub binding: Option<MidiBindingDto>,
    /// Readable form of the first captured message.
    pub description: String,
    /// Why it was not applied (input already used, nothing captured).
    pub error: Option<String>,
}

/// Where the browse encoder moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowseSourceDto {
    /// The collection with the query the library pane shows.
    Collection {
        /// Filter and sort; offset and limit are ignored.
        query: TrackQueryDto,
    },
    /// A playlist in order.
    Playlist {
        /// Playlist.
        id: PlaylistId,
    },
}

/// What the library pane tells the browse cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowseContextDto {
    /// The list on screen.
    pub source: BrowseSourceDto,
    /// Selected row, `None` when nothing is selected.
    pub index: Option<u32>,
}

/// The browse cursor moved (encoder turn); the pane selects the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct BrowseCursorDto {
    /// Row in the current list.
    pub index: u32,
    /// Track at that row.
    pub track: TrackId,
}

/// A pad mode changed from the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PadModeChanged {
    /// Deck.
    pub deck: DeckId,
    /// New mode.
    pub mode: PadMode,
}
