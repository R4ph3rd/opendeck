//! The control-thread half of the engine: sends commands and tracks, reads
//! telemetry and events, drops buffers the audio thread has returned.

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::telemetry::{EngineEvent, EngineTelemetry};
use opendeck_contracts::{DeckId, EngineCommand, Frames, SampleRate};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio::{CoreRings, EngineCore};
use crate::track::{Handoff, TrackBuffer};
use crate::{ConfigError, EngineConfig};

/// Capacity of the command ring (ADR 0003).
pub const COMMAND_RING: usize = 1024;
/// Capacity of the track hand-off ring.
pub const HANDOFF_RING: usize = 16;
/// Capacity of the return ring for replaced buffers.
pub const RETURN_RING: usize = 64;
/// Capacity of the telemetry ring.
pub const TELEMETRY_RING: usize = 4;
/// Capacity of the event ring.
pub const EVENT_RING: usize = 256;

/// The command ring was full; the command was not queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("engine command ring is full")]
pub struct CommandRingFull;

/// Control-side endpoints of the engine rings. Owned by one thread.
pub struct EngineHandle {
    commands: Producer<EngineCommand>,
    handoff: Producer<Handoff>,
    returns: Consumer<TrackBuffer>,
    telemetry: Consumer<EngineTelemetry>,
    events: Consumer<EngineEvent>,
    dropped_commands: u32,
    sample_rate: SampleRate,
}

/// Builds the audio-side [`EngineCore`] and the control-side [`EngineHandle`]
/// for `config`. Allocates every buffer; call before the stream starts.
pub fn engine(config: EngineConfig) -> Result<(EngineCore, EngineHandle), ConfigError> {
    engine_with_xruns(config, Arc::new(AtomicU32::new(0)))
}

/// Like [`engine`], with a shared counter the device layer bumps on xruns.
/// The core reports it in telemetry.
pub fn engine_with_xruns(
    config: EngineConfig,
    xruns: Arc<AtomicU32>,
) -> Result<(EngineCore, EngineHandle), ConfigError> {
    config.validate()?;
    let (commands_tx, commands_rx) = RingBuffer::new(COMMAND_RING);
    let (handoff_tx, handoff_rx) = RingBuffer::new(HANDOFF_RING);
    let (returns_tx, returns_rx) = RingBuffer::new(RETURN_RING);
    let (telemetry_tx, telemetry_rx) = RingBuffer::new(TELEMETRY_RING);
    let (events_tx, events_rx) = RingBuffer::new(EVENT_RING);
    let core = EngineCore::new(
        config,
        CoreRings {
            commands: commands_rx,
            handoff: handoff_rx,
            returns: returns_tx,
            telemetry: telemetry_tx,
            events: events_tx,
        },
        xruns,
    );
    let handle = EngineHandle {
        commands: commands_tx,
        handoff: handoff_tx,
        returns: returns_rx,
        telemetry: telemetry_rx,
        events: events_rx,
        dropped_commands: 0,
        sample_rate: config.sample_rate,
    };
    Ok((core, handle))
}

impl EngineHandle {
    /// Engine sample rate; converts every `Frames` value to seconds.
    pub fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Queues a command. On a full ring the command is dropped, counted in
    /// [`dropped_commands`](Self::dropped_commands) and an error is returned
    /// so the caller can retry on its next tick.
    pub fn send(&mut self, command: EngineCommand) -> Result<(), CommandRingFull> {
        self.commands.push(command).map_err(|_| {
            self.dropped_commands = self.dropped_commands.saturating_add(1);
            CommandRingFull
        })
    }

    /// Hands a decoded track to the audio thread for `deck`, without a beat
    /// grid. See [`load_track_with_grid`](Self::load_track_with_grid).
    pub fn load_track(
        &mut self,
        deck: DeckId,
        track: TrackBuffer,
        cue_point: Option<Frames>,
    ) -> Result<(), TrackBuffer> {
        self.load_track_with_grid(deck, track, cue_point, None)
    }

    /// Hands a decoded track and its beat grid to the audio thread for `deck`.
    /// The deck stops, rewinds to the start and sets its cue point to
    /// `cue_point` (start of the track when `None`). Loops, hot cues and slip
    /// of the previous track are cleared: send `SetHotCue` again after the
    /// load. Sync, quantize, slip mode, vinyl mode, key lock and tempo
    /// settings stay. Any previous buffer comes back through
    /// [`reclaim`](Self::reclaim).
    ///
    /// `grid` is in seconds from the first frame of `track`. Without one the
    /// deck cannot sync, auto loop, roll or beat jump.
    ///
    /// Returns the buffer when the hand-off ring is full so the caller can retry.
    pub fn load_track_with_grid(
        &mut self,
        deck: DeckId,
        track: TrackBuffer,
        cue_point: Option<Frames>,
        grid: Option<BeatGrid>,
    ) -> Result<(), TrackBuffer> {
        // Cloning shares the samples; it only keeps a handle to give back.
        let backup = track.clone();
        self.handoff
            .push(Handoff::Track {
                deck,
                track,
                cue_point,
                grid,
            })
            .map_err(|_| backup)
    }

    /// Sets or clears the beat grid of the track loaded in `deck` (analysis
    /// finished after the load, or the grid was edited). Returns `false` when
    /// the hand-off ring is full; retry on the next tick.
    pub fn set_beat_grid(&mut self, deck: DeckId, grid: Option<BeatGrid>) -> bool {
        self.handoff.push(Handoff::Grid { deck, grid }).is_ok()
    }

    /// Drops every buffer the audio thread has returned and reports how many.
    /// Call from the control or loader thread, never from the audio thread.
    pub fn reclaim(&mut self) -> usize {
        let mut dropped = 0;
        while let Ok(buffer) = self.returns.pop() {
            drop(buffer);
            dropped += 1;
        }
        dropped
    }

    /// Newest telemetry snapshot since the last call, skipping older ones.
    pub fn poll_telemetry(&mut self) -> Option<EngineTelemetry> {
        let mut latest = None;
        while let Ok(snapshot) = self.telemetry.pop() {
            latest = Some(snapshot);
        }
        latest
    }

    /// Next event from the audio thread, oldest first.
    pub fn poll_event(&mut self) -> Option<EngineEvent> {
        self.events.pop().ok()
    }

    /// Commands rejected because the ring was full since the engine started.
    pub fn dropped_commands(&self) -> u32 {
        self.dropped_commands
    }
}
