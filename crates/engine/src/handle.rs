//! The control-thread half of the engine: sends commands and tracks, reads
//! telemetry and events, drops buffers the audio thread has returned.

use std::sync::Arc;
use std::sync::atomic::AtomicU32;

use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::fx::Effect;
use opendeck_contracts::ids::FX_SLOTS_PER_UNIT;
use opendeck_contracts::telemetry::{EngineEvent, EngineTelemetry};
use opendeck_contracts::{
    Bpm, DeckId, EngineCommand, Frames, FxSlotId, FxUnit, SampleRate, SamplerSlotId,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::audio::{CoreRings, EngineCore};
use crate::fx::{FX_UNITS, FxEvent, FxHandoff, FxHost, FxTempoSource, SendTap, SlotRt};
use crate::track::{Handoff, TrackBuffer};
use crate::{ConfigError, EngineConfig};

/// Capacity of the command ring (ADR 0003).
pub const COMMAND_RING: usize = 1024;
/// Capacity of the track hand-off ring.
pub const HANDOFF_RING: usize = 16;
/// Capacity of the return ring for replaced buffers.
pub const RETURN_RING: usize = 64;
/// Capacity of the FX hand-off ring (effects, timed events, tempo and send
/// taps): one install for every slot of every unit fits at once.
pub const FX_HANDOFF_RING: usize = FX_UNITS * FX_SLOTS_PER_UNIT;
/// Capacity of the FX return ring for replaced effects: two per slot.
pub const FX_RETURN_RING: usize = 2 * FX_UNITS * FX_SLOTS_PER_UNIT;
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
    fx_handoff: Producer<FxHandoff>,
    fx_returns: Consumer<Box<SlotRt>>,
    telemetry: Consumer<EngineTelemetry>,
    events: Consumer<EngineEvent>,
    dropped_commands: u32,
    sample_rate: SampleRate,
    max_block_frames: usize,
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
    let (fx_handoff_tx, fx_handoff_rx) = RingBuffer::new(FX_HANDOFF_RING);
    let (fx_returns_tx, fx_returns_rx) = RingBuffer::new(FX_RETURN_RING);
    let (telemetry_tx, telemetry_rx) = RingBuffer::new(TELEMETRY_RING);
    let (events_tx, events_rx) = RingBuffer::new(EVENT_RING);
    let core = EngineCore::new(
        config,
        CoreRings {
            commands: commands_rx,
            handoff: handoff_rx,
            returns: returns_tx,
            fx_handoff: fx_handoff_rx,
            fx_returns: fx_returns_tx,
            telemetry: telemetry_tx,
            events: events_tx,
        },
        xruns,
    );
    let handle = EngineHandle {
        commands: commands_tx,
        handoff: handoff_tx,
        returns: returns_rx,
        fx_handoff: fx_handoff_tx,
        fx_returns: fx_returns_rx,
        telemetry: telemetry_rx,
        events: events_rx,
        dropped_commands: 0,
        sample_rate: config.sample_rate,
        max_block_frames: config.max_block_frames as usize,
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

    /// Hands a decoded sample to the audio thread for sampler `slot`. Same
    /// buffer type and rings as [`load_track`](Self::load_track): interleaved
    /// stereo `f32` at the engine rate; a replaced sample comes back through
    /// [`reclaim`](Self::reclaim). A sound playing in the slot is cut. Gain,
    /// mode and sync of the slot stay.
    ///
    /// `bpm` is the tempo of the sample, needed for `SetSamplerSync`; a
    /// non-finite or non-positive value counts as `None`.
    ///
    /// Returns the buffer when the hand-off ring is full so the caller can retry.
    pub fn load_sampler(
        &mut self,
        slot: SamplerSlotId,
        track: TrackBuffer,
        bpm: Option<Bpm>,
    ) -> Result<(), TrackBuffer> {
        let backup = track.clone();
        self.handoff
            .push(Handoff::Sampler { slot, track, bpm })
            .map_err(|_| backup)
    }

    /// Empties sampler `slot`; the old sample comes back through
    /// [`reclaim`](Self::reclaim). Returns `false` when the hand-off ring is
    /// full; retry on the next tick.
    pub fn clear_sampler(&mut self, slot: SamplerSlotId) -> bool {
        self.handoff.push(Handoff::SamplerClear { slot }).is_ok()
    }

    /// Installs `effect` into `slot`, replacing the effect there (a hard cut:
    /// disable the slot and let it settle first to avoid a click).
    ///
    /// Control or loader thread. The effect comes from
    /// `EffectRegistry::create`; this prepares it for the engine (sample rate,
    /// block size, `Effect::prepare`), allocates the slot buffers and hands
    /// the box over a dedicated ring, so the audio thread never allocates.
    /// Set the effect's initial parameters before calling (`set_parameter`
    /// is safe on any thread). The slot keeps its enabled state, wet/dry and
    /// beat division from `SetFxEnabled`, `SetFxMix` and `SetFxBeatDivision`.
    /// Latency is read once here (see the `fx` module docs).
    ///
    /// Returns the effect when the hand-off ring is full; retry on the next tick.
    pub fn install_effect(
        &mut self,
        slot: FxSlotId,
        effect: Box<dyn Effect>,
    ) -> Result<(), Box<dyn Effect>> {
        if self.fx_handoff.is_full() {
            return Err(effect);
        }
        let rt = Box::new(SlotRt::new(effect, self.sample_rate, self.max_block_frames));
        let item = FxHandoff::Install {
            unit: FxHost::unit_index(slot.unit()),
            slot: slot.slot(),
            rt,
        };
        match self.fx_handoff.push(item) {
            Err(rtrb::PushError::Full(FxHandoff::Install { rt, .. })) => Err((*rt).into_effect()),
            // Only this handle pushes and the ring was not full.
            _ => Ok(()),
        }
    }

    /// Empties `slot`. The old effect comes back through [`reclaim`](Self::reclaim).
    /// Returns `false` when the hand-off ring is full; retry on the next tick.
    pub fn clear_effect(&mut self, slot: FxSlotId) -> bool {
        self.fx_handoff
            .push(FxHandoff::Clear {
                unit: FxHost::unit_index(slot.unit()),
                slot: slot.slot(),
            })
            .is_ok()
    }

    /// Chooses where send bus `bus` takes its signal from every channel
    /// (default post-fader). Returns `false` for a bad bus or a full ring.
    pub fn set_fx_send_tap(&mut self, bus: u8, tap: SendTap) -> bool {
        usize::from(bus) < opendeck_contracts::ids::FX_SEND_BUSES
            && self
                .fx_handoff
                .push(FxHandoff::SendTap { bus, tap })
                .is_ok()
    }

    /// Sets where `unit` takes tempo and beat phase from: its usual source,
    /// a given deck, or a tapped BPM (see [`FxTempoSource`]). Returns `false`
    /// for a tapped BPM that is not finite and positive, or when the hand-off
    /// ring is full; retry on the next tick.
    pub fn set_fx_tempo_source(&mut self, unit: FxUnit, source: FxTempoSource) -> bool {
        if let FxTempoSource::Tap(bpm) = source
            && !(bpm.0.is_finite() && bpm.0 > 0.0)
        {
            return false;
        }
        self.fx_handoff
            .push(FxHandoff::Tempo {
                unit: FxHost::unit_index(unit),
                source,
            })
            .is_ok()
    }

    /// Enables or disables `slot` at engine frame `at` (the
    /// `EngineTelemetry::engine_time` clock), sample accurate: the slot starts
    /// its ramp on exactly that frame. `fast` uses a 1 ms ramp (pad FX engage
    /// and release) instead of the 10 ms bypass crossfade. A time already
    /// past acts on the first frame of the next block. Up to 48 timed events
    /// (`2 * MAX_DECKS * FX_SLOTS_PER_UNIT`) wait at once (more act
    /// immediately). Returns `false` when the ring is
    /// full; retry on the next tick.
    pub fn schedule_fx_enabled(
        &mut self,
        slot: FxSlotId,
        on: bool,
        at: Frames,
        fast: bool,
    ) -> bool {
        self.fx_handoff
            .push(FxHandoff::Event(FxEvent {
                unit: FxHost::unit_index(slot.unit()),
                slot: slot.slot(),
                on,
                at,
                fast,
            }))
            .is_ok()
    }

    /// Release FX: at frame `at`, bypasses `from` (10 ms crossfade) and
    /// starts `release` with a 1 ms ramp; `hold` frames later `release` is
    /// disabled again, so an effect with a tail (echo out, reverb) keeps
    /// ringing after its input is cut while a non-tail effect (brake, filter
    /// sweep) is simply bypassed. Place `release` after `from` in the chain
    /// so it hears what `from` was producing. All three switches are queued
    /// together or none are (`false`, ring too full; retry).
    pub fn release_fx(
        &mut self,
        from: FxSlotId,
        release: FxSlotId,
        at: Frames,
        hold: Frames,
    ) -> bool {
        if self.fx_handoff.slots() < 3 {
            return false;
        }
        let end = Frames(at.0.saturating_add(hold.0));
        self.schedule_fx_enabled(from, false, at, false)
            && self.schedule_fx_enabled(release, true, at, true)
            && self.schedule_fx_enabled(release, false, end, false)
    }

    /// Drops every buffer and effect the audio thread has returned and
    /// reports how many. Call from the control or loader thread, never from
    /// the audio thread.
    pub fn reclaim(&mut self) -> usize {
        let mut dropped = 0;
        while let Ok(buffer) = self.returns.pop() {
            drop(buffer);
            dropped += 1;
        }
        while let Ok(old) = self.fx_returns.pop() {
            drop(old);
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
