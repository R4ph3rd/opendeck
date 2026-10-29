//! The audio-thread half of the engine: command dispatch, decks, master bus
//! and telemetry. Driven by a device callback or by the offline renderer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use opendeck_contracts::telemetry::{EngineEvent, EngineTelemetry};
use opendeck_contracts::{DeckId, EngineCommand, MAX_DECKS};
use rtrb::{Consumer, Producer};

use crate::EngineConfig;
use crate::deck::{Deck, DeckStatus, EventSink};
use crate::mixer::Mixer;
use crate::sync::SyncState;
use crate::track::{Handoff, TrackBuffer};

/// Telemetry snapshots per second.
const TELEMETRY_HZ: u32 = 60;

/// Hand-off items applied per block at most.
const HANDOFF_PER_BLOCK: usize = 4;

/// Slots for buffers waiting for room on the return ring.
const PENDING_RETURNS: usize = 2 * MAX_DECKS;

/// Smoothing factor for the reported DSP load (per block).
const LOAD_SMOOTHING: f32 = 0.9;

/// Audio-thread engine state. Built by [`engine`](crate::engine), then moved
/// into the device callback or an [`OfflineRenderer`](crate::OfflineRenderer).
pub struct EngineCore {
    config: EngineConfig,
    decks: Vec<Deck>,
    mixer: Mixer,
    sync: SyncState,

    commands: Consumer<EngineCommand>,
    handoff: Consumer<Handoff>,
    returns: Producer<TrackBuffer>,
    pending_returns: [Option<TrackBuffer>; PENDING_RETURNS],
    telemetry: Producer<EngineTelemetry>,
    sink: EventSink,

    deck_buf: Vec<f32>,
    /// Headphone bus for callers that only want the master.
    hp_scratch: Vec<f32>,

    sequence: u64,
    engine_time: u64,
    frames_since_publish: u32,
    publish_interval: u32,
    dsp_load: f32,
    xruns: Arc<AtomicU32>,
}

/// Ring endpoints the audio side owns, handed to [`EngineCore::new`].
pub(crate) struct CoreRings {
    pub(crate) commands: Consumer<EngineCommand>,
    pub(crate) handoff: Consumer<Handoff>,
    pub(crate) returns: Producer<TrackBuffer>,
    pub(crate) telemetry: Producer<EngineTelemetry>,
    pub(crate) events: Producer<EngineEvent>,
}

impl EngineCore {
    /// Allocates every buffer the callback will use. Not real-time safe.
    pub(crate) fn new(config: EngineConfig, rings: CoreRings, xruns: Arc<AtomicU32>) -> Self {
        let max_block = config.max_block_frames as usize;
        let decks = (0..config.deck_count)
            .filter_map(DeckId::new)
            .map(|id| Deck::new(id, config.sample_rate, max_block, config.interpolation))
            .collect();
        Self {
            config,
            decks,
            mixer: Mixer::new(config.sample_rate.as_f64(), max_block),
            sync: SyncState::new(config.sample_rate.as_f64()),
            commands: rings.commands,
            handoff: rings.handoff,
            returns: rings.returns,
            pending_returns: Default::default(),
            telemetry: rings.telemetry,
            sink: EventSink {
                producer: rings.events,
                dropped: 0,
            },
            deck_buf: vec![0.0; max_block * 2],
            hp_scratch: vec![0.0; max_block * 2],
            sequence: 0,
            engine_time: 0,
            frames_since_publish: 0,
            publish_interval: (config.sample_rate.0 / TELEMETRY_HZ).max(1),
            dsp_load: 0.0,
            xruns,
        }
    }

    /// The configuration this core was built with.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Transport state of `deck` as of the last rendered block, `None` if the
    /// deck is not allocated. Same-thread read for offline tools and tests;
    /// the control thread uses telemetry instead.
    pub fn deck_status(&self, deck: DeckId) -> Option<DeckStatus> {
        self.decks.get(deck.index()).map(Deck::status)
    }

    /// Renders interleaved stereo master output into `out`, overwriting it.
    /// The headphone bus is computed and dropped; see
    /// [`render_with_headphones`](Self::render_with_headphones).
    ///
    /// `out.len()` must be even; a trailing odd sample is zeroed. Any length is
    /// accepted: the render is split into chunks of at most
    /// `max_block_frames`. Commands queued before the call are applied at
    /// chunk boundaries. Real-time safe; wrapped in `assert_no_alloc` when the
    /// `alloc-guard` feature is on and in tests.
    pub fn render(&mut self, out: &mut [f32]) {
        crate::rt_guard(|| self.render_inner(out, None));
    }

    /// Like [`render`](Self::render), and also renders the headphone bus
    /// (cue and master blend, or split cue) into `headphones`, interleaved
    /// stereo of the same length. A shorter `headphones` is filled as far as
    /// it goes.
    pub fn render_with_headphones(&mut self, master: &mut [f32], headphones: &mut [f32]) {
        crate::rt_guard(|| self.render_inner(master, Some(headphones)));
    }

    fn render_inner(&mut self, out: &mut [f32], mut headphones: Option<&mut [f32]>) {
        let max_block = (self.config.max_block_frames as usize).max(1);
        let (body, tail) = out.split_at_mut(out.len() & !1);
        tail.fill(0.0);
        if let Some(hp) = headphones.as_deref_mut() {
            let odd = hp.len() & 1;
            if let Some(last) = hp.len().checked_sub(odd).and_then(|at| hp.get_mut(at..)) {
                last.fill(0.0);
            }
        }
        let mut scratch = std::mem::take(&mut self.hp_scratch);
        let mut offset = 0;
        for chunk in body.chunks_mut(max_block * 2) {
            let started = Instant::now();
            let len = chunk.len();
            let wanted = headphones
                .as_deref_mut()
                .and_then(|hp| hp.get_mut(offset..offset + len));
            match wanted {
                Some(hp) => self.render_chunk(chunk, hp),
                None => {
                    if let Some(hp) = scratch.get_mut(..len) {
                        self.render_chunk(chunk, hp);
                    }
                }
            }
            offset += len;
            let frames = len / 2;
            let budget = frames as f32 / self.config.sample_rate.0.max(1) as f32;
            let load = started.elapsed().as_secs_f32() / budget.max(f32::EPSILON);
            self.dsp_load = self.dsp_load * LOAD_SMOOTHING + load * (1.0 - LOAD_SMOOTHING);
        }
        self.hp_scratch = scratch;
    }

    fn render_chunk(&mut self, out: &mut [f32], headphones: &mut [f32]) {
        let frames = out.len() / 2;
        self.flush_pending_returns();
        self.drain_handoff();
        self.drain_commands();
        self.sync.update(&mut self.decks);

        out.fill(0.0);
        self.mixer.begin_block(frames);
        for (index, deck) in self.decks.iter_mut().enumerate() {
            let Some(buf) = self.deck_buf.get_mut(..frames * 2) else {
                continue;
            };
            deck.render(buf, &mut self.sink);
            self.mixer.process_deck(index, buf, out);
        }
        self.mixer.finish(out, headphones);

        self.engine_time = self.engine_time.saturating_add(frames as u64);
        self.frames_since_publish = self.frames_since_publish.saturating_add(frames as u32);
        if self.frames_since_publish >= self.publish_interval {
            self.publish();
        }
    }

    fn publish(&mut self) {
        // Keep the remainder so the average rate stays at TELEMETRY_HZ whatever the block size.
        self.frames_since_publish %= self.publish_interval;
        let mut snapshot = EngineTelemetry {
            sequence: self.sequence,
            engine_time: opendeck_contracts::Frames(self.engine_time),
            sample_rate: self.config.sample_rate,
            master: self.mixer.master_meter.take(),
            headphones: self.mixer.headphone_meter.take(),
            leader: self.sync.leader(),
            dsp_load: self.dsp_load,
            xruns: self.xruns.load(Ordering::Relaxed),
            dropped_events: self.sink.dropped,
            ..EngineTelemetry::default()
        };
        for (index, (slot, deck)) in snapshot.decks.iter_mut().zip(&self.decks).enumerate() {
            *slot = deck.snapshot();
            slot.meter = self.mixer.take_channel_meter(index);
        }
        // A full ring means the reader is behind; the next snapshot is complete anyway.
        if self.telemetry.push(snapshot).is_ok() {
            self.sequence = self.sequence.wrapping_add(1);
        }
    }

    // ----- Rings -----

    /// True when a replaced buffer can be handed back without dropping it here.
    fn can_return(&self) -> bool {
        self.returns.slots() > 0 || self.pending_returns.iter().any(Option::is_none)
    }

    /// Hands a buffer to the return ring, or parks it until there is room.
    /// The audio thread never frees a track buffer.
    fn return_buffer(&mut self, buffer: TrackBuffer) {
        let buffer = match self.returns.push(buffer) {
            Ok(()) => return,
            Err(rtrb::PushError::Full(buffer)) => buffer,
        };
        match self.pending_returns.iter_mut().find(|slot| slot.is_none()) {
            Some(slot) => *slot = Some(buffer),
            // `can_return` is checked before every replacement, so this is
            // unreachable. Leaking beats freeing on the audio thread.
            None => std::mem::forget(buffer),
        }
    }

    fn flush_pending_returns(&mut self) {
        for slot in &mut self.pending_returns {
            if self.returns.slots() == 0 {
                break;
            }
            if let Some(buffer) = slot.take()
                && let Err(rtrb::PushError::Full(buffer)) = self.returns.push(buffer)
            {
                *slot = Some(buffer);
            }
        }
    }

    fn drain_handoff(&mut self) {
        for _ in 0..HANDOFF_PER_BLOCK {
            if !self.can_return() || self.handoff.is_empty() {
                return;
            }
            match self.handoff.pop() {
                Ok(Handoff::Track {
                    deck,
                    track,
                    cue_point,
                    grid,
                }) => {
                    let replaced = match self.decks.get_mut(deck.index()) {
                        Some(target) => target.load(track, cue_point, grid),
                        // Deck not allocated: give the buffer straight back.
                        None => Some(track),
                    };
                    if let Some(old) = replaced {
                        self.return_buffer(old);
                    }
                }
                Ok(Handoff::Grid { deck, grid }) => {
                    if let Some(target) = self.decks.get_mut(deck.index()) {
                        target.set_grid(grid);
                    }
                }
                Err(_) => return,
            }
        }
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            self.apply(command);
        }
    }

    fn apply(&mut self, command: EngineCommand) {
        use EngineCommand as C;
        if self.mixer.apply(command) {
            return;
        }
        if let C::SetLeader { deck } = command {
            self.sync.set_preferred(deck);
            return;
        }
        if let C::Eject { deck } = command {
            if !self.can_return() {
                // Retrying later is the control side's job; count it as dropped.
                self.sink.dropped = self.sink.dropped.saturating_add(1);
                return;
            }
            let old = self.decks.get_mut(deck.index()).and_then(Deck::eject);
            if let Some(old) = old {
                self.return_buffer(old);
            }
            return;
        }
        let Some(index) = deck_of(&command) else {
            // TODO(M5): FX. TODO(M7): sampler, key shift.
            return;
        };
        let Some(d) = self.decks.get_mut(index) else {
            return;
        };
        let sink = &mut self.sink;
        match command {
            C::SetPlaying { playing, .. } => d.set_playing(playing),
            C::Cue { pressed, .. } => d.cue(pressed, sink),
            C::SetCuePoint { position, .. } => d.set_cue_point(position),
            C::Seek { position, .. } => d.seek(position),
            C::SetTempoFader { position, .. } => d.set_tempo_fader(position),
            C::SetTempoRange { range, .. } => d.set_tempo_range(range),
            C::SetKeyLock { on, .. } => d.set_key_lock(on),
            C::PitchBend { amount, .. } => d.set_pitch_bend(amount),
            C::SetSync { mode, .. } => d.set_sync_mode(mode),
            C::JogTouch { touched, .. } => d.jog_touch(touched),
            C::JogTurn { revolutions, .. } => d.jog_turn(revolutions),
            C::JogSearch { beats, .. } => d.jog_search(beats),
            C::SetVinylMode { on, .. } => d.set_vinyl(on),
            C::LoopIn { .. } => d.loop_in(),
            C::LoopOut { .. } => d.loop_out(sink),
            C::LoopToggle { .. } => d.loop_toggle(),
            C::AutoLoop { length, .. } => d.auto_loop(length, sink),
            C::LoopRoll { length, active, .. } => d.loop_roll(length, active),
            C::LoopResize { factor, .. } => d.loop_resize(factor, sink),
            C::SetLoop { region, .. } => d.set_loop(region),
            C::BeatJump { beats, .. } => d.beat_jump(beats),
            C::SetHotCue { index, cue, .. } => d.set_hot_cue(index, cue),
            C::HotCue { index, pressed, .. } => d.hot_cue(index, pressed, sink),
            C::SetSlip { on, .. } => d.set_slip(on),
            C::SetQuantize { on, .. } => d.set_quantize(on),
            _ => {}
        }
    }
}

/// Index of the deck a deck command addresses.
fn deck_of(command: &EngineCommand) -> Option<usize> {
    use EngineCommand as C;
    match command {
        C::SetPlaying { deck, .. }
        | C::Cue { deck, .. }
        | C::SetCuePoint { deck, .. }
        | C::Seek { deck, .. }
        | C::SetTempoFader { deck, .. }
        | C::SetTempoRange { deck, .. }
        | C::SetKeyLock { deck, .. }
        | C::PitchBend { deck, .. }
        | C::SetSync { deck, .. }
        | C::JogTouch { deck, .. }
        | C::JogTurn { deck, .. }
        | C::JogSearch { deck, .. }
        | C::SetVinylMode { deck, .. }
        | C::LoopIn { deck }
        | C::LoopOut { deck }
        | C::LoopToggle { deck }
        | C::AutoLoop { deck, .. }
        | C::LoopRoll { deck, .. }
        | C::LoopResize { deck, .. }
        | C::SetLoop { deck, .. }
        | C::BeatJump { deck, .. }
        | C::SetHotCue { deck, .. }
        | C::HotCue { deck, .. }
        | C::SetSlip { deck, .. }
        | C::SetQuantize { deck, .. } => Some(deck.index()),
        _ => None,
    }
}
