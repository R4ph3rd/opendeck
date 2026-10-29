//! Deck state snapshot shared between app-core and the UI.
//!
//! Fast-moving values (playhead, meters, beat phase) are not here: they come
//! from [`EngineTelemetry`](crate::telemetry::EngineTelemetry) at ~60 Hz.
//! `DeckState` changes on user actions and track loads only.

use crate::beatgrid::BeatGrid;
use crate::cue::LoopRegion;
use crate::ids::{DeckId, TrackId};
use crate::key::MusicalKey;
use crate::pad::PadMode;
use crate::tempo::{SyncMode, TempoRange};
use crate::units::{Bpm, Seconds};

/// Transport state of a deck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PlayState {
    /// No track loaded.
    #[default]
    Empty,
    /// Track is being decoded by the loader thread.
    Loading,
    /// Loaded and stopped.
    Paused,
    /// Playing.
    Playing,
    /// CUE held while paused: playing from the cue point until release.
    CuePreview,
}

/// Snapshot of one deck's control state. Not RT (owns no heap today, but is
/// not part of the RT contract and may grow).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeckState {
    /// Which deck.
    pub deck: DeckId,
    /// Loaded track, if any.
    pub track: Option<TrackId>,
    /// Transport state.
    pub play_state: PlayState,
    /// Main cue point.
    pub cue_point: Option<Seconds>,
    /// Track tempo at original speed, from analysis.
    pub track_bpm: Option<Bpm>,
    /// Track key, from analysis.
    pub track_key: Option<MusicalKey>,
    /// Beat grid, from analysis.
    pub beat_grid: Option<BeatGrid>,
    /// Tempo fader range.
    pub tempo_range: TempoRange,
    /// Tempo fader position in `[-1, 1]` (`+1` = fastest).
    pub tempo_fader: f32,
    /// Key lock (master tempo): tempo changes keep pitch.
    pub key_lock: bool,
    /// Key shift in semitones applied on top of the track key.
    pub key_shift: i8,
    /// Sync mode.
    pub sync: SyncMode,
    /// True if this deck is the sync leader (election: `EngineCommand::SetLeader`).
    pub is_leader: bool,
    /// Slip mode on.
    pub slip: bool,
    /// Quantize on: cue, hot cue and loop points snap to the nearest beat, a
    /// hot cue jump waits for the next beat. See `EngineCommand::SetQuantize`.
    pub quantize: bool,
    /// Jog in vinyl mode: platter touch scratches. Otherwise it bends pitch.
    pub vinyl_mode: bool,
    /// Active loop, if looping.
    pub active_loop: Option<LoopRegion>,
    /// Pad mode shown by the pads and the middle UI panel.
    pub pad_mode: PadMode,
}

impl DeckState {
    /// State of an empty deck with default settings.
    pub fn empty(deck: DeckId) -> Self {
        Self {
            deck,
            track: None,
            play_state: PlayState::Empty,
            cue_point: None,
            track_bpm: None,
            track_key: None,
            beat_grid: None,
            tempo_range: TempoRange::default(),
            tempo_fader: 0.0,
            key_lock: false,
            key_shift: 0,
            sync: SyncMode::Off,
            is_leader: false,
            slip: false,
            quantize: true,
            vinyl_mode: true,
            active_loop: None,
            pad_mode: PadMode::HotCue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_deck_defaults() {
        let deck = DeckId::new(1).expect("deck 2");
        let state = DeckState::empty(deck);
        assert_eq!(state.play_state, PlayState::Empty);
        assert_eq!(state.pad_mode, PadMode::HotCue);
        assert_eq!(state.tempo_range, TempoRange::Percent10);
        assert!(state.vinyl_mode, "FLX4 vinyl mode defaults to ON");
    }
}
