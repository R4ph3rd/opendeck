//! What the audio thread reports back.
//!
//! [`EngineTelemetry`] is a full snapshot written by the audio thread at most
//! once per block into a single-slot or SPSC ring; the control thread reads
//! the latest one and forwards it to the UI at ~60 Hz (ADR 0003, ADR 0004).
//! Missing snapshots are fine: each one is complete.
//!
//! [`EngineEvent`] is a rare, must-not-drop notification (a hot cue was set,
//! a track ended) on its own SPSC ring. If that ring is full the audio thread
//! drops the event and counts it in [`EngineTelemetry::dropped_events`].
//! Device variants ([`EngineEvent::DeviceLost`] and friends) come from the
//! engine's device layer on the control thread, since a dead stream has no
//! audio thread left to report them.

use crate::MAX_DECKS;
use crate::cue::{FrameRegion, HotCueIndex};
use crate::ids::{DeckId, FX_SEND_BUSES};
use crate::tempo::SyncMode;
use crate::units::{Bpm, Frames, SampleRate};

/// Peak and RMS levels for a stereo signal over the last block(s), linear
/// amplitude (`1.0` = full scale). RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StereoMeter {
    /// Left peak, held and decayed by the engine for display.
    pub peak_l: f32,
    /// Right peak.
    pub peak_r: f32,
    /// Left RMS.
    pub rms_l: f32,
    /// Right RMS.
    pub rms_r: f32,
}

/// Per-deck fast-moving state. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeckTelemetry {
    /// A track buffer is loaded.
    pub loaded: bool,
    /// Transport is running (including cue preview and loop roll).
    pub playing: bool,
    /// Playhead in engine frames.
    pub position: Frames,
    /// Track length in engine frames.
    pub duration: Frames,
    /// Where playback would be without slip mode, when slip is engaged.
    pub slip_position: Option<Frames>,
    /// Current playback rate (tempo fader, sync, bend and scratch combined).
    pub rate: f64,
    /// Effective tempo (grid BPM times rate, sync included), if the track
    /// has a grid.
    pub bpm: Option<Bpm>,
    /// Position inside the current beat, `[0, 1)`; 0 when there is no grid.
    pub beat_phase: f32,
    /// Zero-based beat within the bar; 0 when there is no grid.
    pub beat_in_bar: u8,
    /// Sync mode the engine applies to this deck.
    pub sync_mode: SyncMode,
    /// This deck is the sync leader: `EngineTelemetry::leader` names it.
    /// A leader can have sync off.
    pub is_leader: bool,
    /// Current loop in engine frames: the playing one, or the last one that
    /// `LoopToggle` re-enters. Cleared on track load.
    pub loop_region: Option<FrameRegion>,
    /// The playhead is looping inside `loop_region` (loop, hot loop or roll).
    pub loop_active: bool,
    /// Post-fader channel level.
    pub meter: StereoMeter,
}

/// State of one FX unit (a chain of slots) over the last block. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxUnitTelemetry {
    /// At least one slot holding an effect is enabled.
    pub enabled: bool,
    /// At least one slot is processing: enabled, crossfading, or ringing out
    /// its tail after being disabled.
    pub active: bool,
    /// Peak of the unit's output over the last block it ran, linear
    /// amplitude; 0 while idle.
    pub level: f32,
}

/// Full engine snapshot. RT-safe: `Copy`, fixed size, written without allocation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EngineTelemetry {
    /// Increments by one per published snapshot; lets readers detect gaps.
    pub sequence: u64,
    /// Frames rendered since the stream started (engine clock).
    pub engine_time: Frames,
    /// Engine sample rate; converts every `Frames` here to seconds.
    pub sample_rate: SampleRate,
    /// One entry per deck, indexed by [`DeckId::index`].
    pub decks: [DeckTelemetry; MAX_DECKS],
    /// Master output level.
    pub master: StereoMeter,
    /// Headphone output level.
    pub headphones: StereoMeter,
    /// Deck FX units (insert chains), indexed by [`DeckId::index`].
    pub fx_decks: [FxUnitTelemetry; MAX_DECKS],
    /// Master FX unit.
    pub fx_master: FxUnitTelemetry,
    /// Send FX units, indexed by bus.
    pub fx_sends: [FxUnitTelemetry; FX_SEND_BUSES],
    /// Current sync leader. See `EngineCommand::SetLeader` for the election.
    pub leader: Option<DeckId>,
    /// Callback time divided by block duration, smoothed, `0..` (1.0 = overrun).
    pub dsp_load: f32,
    /// Buffer underruns reported by the backend since the stream started.
    pub xruns: u32,
    /// `EngineEvent`s dropped because their ring was full.
    pub dropped_events: u32,
}

/// A discrete notification from the audio thread. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum EngineEvent {
    /// A hot cue was stored from the playhead; app-core persists it.
    HotCueSet {
        /// Deck.
        deck: DeckId,
        /// Pad slot.
        index: HotCueIndex,
        /// Stored region (a hot loop if `end` is set).
        cue: FrameRegion,
    },
    /// The main cue point moved (CUE pressed while paused).
    CuePointSet {
        /// Deck.
        deck: DeckId,
        /// New cue point.
        position: Frames,
    },
    /// A loop was set by loop in/out or auto loop; app-core may save it.
    LoopSet {
        /// Deck.
        deck: DeckId,
        /// Loop region.
        region: FrameRegion,
    },
    /// Playback reached the end of the track and stopped.
    TrackEnded {
        /// Deck.
        deck: DeckId,
    },
    /// The output device went away. The stream is dead; the control thread
    /// opens a new one. Reported by the device layer, not the render path.
    DeviceLost,
    /// The host moved the stream to another device and keeps playing.
    DeviceChanged,
    /// The backend reported some other stream error.
    StreamError,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_is_copy_without_drop() {
        assert!(!core::mem::needs_drop::<EngineTelemetry>());
        assert!(!core::mem::needs_drop::<EngineEvent>());
        let t = EngineTelemetry::default();
        assert_eq!(t.decks.len(), MAX_DECKS);
        assert_eq!(t.fx_decks.len(), MAX_DECKS);
        assert_eq!(t.fx_sends.len(), FX_SEND_BUSES);
        assert_eq!(t.sample_rate, SampleRate(48_000));
    }
}
