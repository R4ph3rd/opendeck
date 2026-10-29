//! The audio-thread half of the engine: command dispatch, decks, master bus
//! and telemetry. Driven by a device callback or by the offline renderer.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use opendeck_contracts::telemetry::{EngineEvent, EngineTelemetry, StereoMeter};
use opendeck_contracts::{DeckId, EngineCommand, MAX_DECKS};
use rtrb::{Consumer, Producer};

use crate::EngineConfig;
use crate::deck::{Deck, DeckStatus, EventSink};
use crate::meter::MeterAcc;
use crate::track::{Handoff, TrackBuffer};

/// Telemetry snapshots per second.
const TELEMETRY_HZ: u32 = 60;

/// Hand-off items applied per block at most.
const HANDOFF_PER_BLOCK: usize = 4;

/// Slots for buffers waiting for room on the return ring.
const PENDING_RETURNS: usize = 2 * MAX_DECKS;

/// Smoothing factor for the reported DSP load (per block).
const LOAD_SMOOTHING: f32 = 0.9;

/// Linear gain that moves to its target over one block.
#[derive(Debug, Clone, Copy)]
struct Ramp {
    current: f32,
    target: f32,
}

impl Ramp {
    fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
        }
    }

    fn set(&mut self, value: f32) {
        if value.is_finite() {
            self.target = value.max(0.0);
        }
    }

    /// Returns `(start, step)` for an `n` frame block and settles on the target.
    fn advance(&mut self, n: usize) -> (f32, f32) {
        let start = self.current;
        let step = (self.target - start) / n.max(1) as f32;
        self.current = self.target;
        (start, step)
    }
}

/// Per-deck mixer state (M1: trim and channel fader only).
#[derive(Debug, Clone, Copy)]
struct Strip {
    trim: f32,
    fader: f32,
    gain: Ramp,
}

impl Strip {
    fn new() -> Self {
        Self {
            trim: 1.0,
            fader: 1.0,
            gain: Ramp::new(1.0),
        }
    }

    fn update(&mut self) {
        self.gain.set(self.trim * self.fader);
    }
}

/// Audio-thread engine state. Built by [`engine`](crate::engine), then moved
/// into the device callback or an [`OfflineRenderer`](crate::OfflineRenderer).
pub struct EngineCore {
    config: EngineConfig,
    decks: Vec<Deck>,
    strips: [Strip; MAX_DECKS],
    master_gain: Ramp,

    commands: Consumer<EngineCommand>,
    handoff: Consumer<Handoff>,
    returns: Producer<TrackBuffer>,
    pending_returns: [Option<TrackBuffer>; PENDING_RETURNS],
    telemetry: Producer<EngineTelemetry>,
    sink: EventSink,

    deck_buf: Vec<f32>,
    master_meter: MeterAcc,

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
            strips: [Strip::new(); MAX_DECKS],
            master_gain: Ramp::new(1.0),
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
            master_meter: MeterAcc::default(),
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
    ///
    /// `out.len()` must be even; a trailing odd sample is zeroed. Any length is
    /// accepted: the render is split into chunks of at most
    /// `max_block_frames`. Commands queued before the call are applied at
    /// chunk boundaries. Real-time safe; wrapped in `assert_no_alloc` when the
    /// `alloc-guard` feature is on and in tests.
    pub fn render(&mut self, out: &mut [f32]) {
        crate::rt_guard(|| self.render_inner(out));
    }

    fn render_inner(&mut self, out: &mut [f32]) {
        let max_block = (self.config.max_block_frames as usize).max(1);
        let (body, tail) = out.split_at_mut(out.len() & !1);
        tail.fill(0.0);
        for chunk in body.chunks_mut(max_block * 2) {
            let started = Instant::now();
            self.render_chunk(chunk);
            let frames = chunk.len() / 2;
            let budget = frames as f32 / self.config.sample_rate.0.max(1) as f32;
            let load = started.elapsed().as_secs_f32() / budget.max(f32::EPSILON);
            self.dsp_load = self.dsp_load * LOAD_SMOOTHING + load * (1.0 - LOAD_SMOOTHING);
        }
    }

    fn render_chunk(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        self.flush_pending_returns();
        self.drain_handoff();
        self.drain_commands();

        out.fill(0.0);
        for (index, deck) in self.decks.iter_mut().enumerate() {
            let Some(buf) = self.deck_buf.get_mut(..frames * 2) else {
                continue;
            };
            deck.render(buf, &mut self.sink);
            let Some(strip) = self.strips.get_mut(index) else {
                continue;
            };
            strip.update();
            let (start, step) = strip.gain.advance(frames);
            for (i, (o, s)) in out.iter_mut().zip(buf.iter()).enumerate() {
                *o += *s * (start + step * ((i / 2 + 1) as f32));
            }
        }

        let (start, step) = self.master_gain.advance(frames);
        for (i, sample) in out.iter_mut().enumerate() {
            let gain = start + step * ((i / 2 + 1) as f32);
            let value = *sample * gain;
            *sample = if value.is_finite() { value } else { 0.0 };
        }
        self.master_meter.add(out);

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
            master: self.master_meter.take(),
            headphones: StereoMeter::default(),
            dsp_load: self.dsp_load,
            xruns: self.xruns.load(Ordering::Relaxed),
            dropped_events: self.sink.dropped,
            ..EngineTelemetry::default()
        };
        for (slot, deck) in snapshot.decks.iter_mut().zip(self.decks.iter_mut()) {
            *slot = deck.snapshot();
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
            let Ok(Handoff::Track {
                deck,
                track,
                cue_point,
            }) = self.handoff.pop()
            else {
                return;
            };
            let replaced = match self.decks.get_mut(deck.index()) {
                Some(target) => target.load(track, cue_point),
                // Deck not allocated: give the buffer straight back.
                None => Some(track),
            };
            if let Some(old) = replaced {
                self.return_buffer(old);
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
        match command {
            C::SetPlaying { deck, playing } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.set_playing(playing);
                }
            }
            C::Cue { deck, pressed } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.cue(pressed, &mut self.sink);
                }
            }
            C::SetCuePoint { deck, position } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.set_cue_point(position);
                }
            }
            C::Seek { deck, position } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.seek(position);
                }
            }
            C::Eject { deck } => {
                if !self.can_return() {
                    // Retrying later is the control side's job; count it as dropped.
                    self.sink.dropped = self.sink.dropped.saturating_add(1);
                    return;
                }
                let old = self.decks.get_mut(deck.index()).and_then(Deck::eject);
                if let Some(old) = old {
                    self.return_buffer(old);
                }
            }
            C::SetTempoFader { deck, position } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.set_tempo_fader(position);
                }
            }
            C::SetTempoRange { deck, range } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.set_tempo_range(range);
                }
            }
            C::SetKeyLock { deck, on } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.set_key_lock(on);
                }
            }
            C::PitchBend { deck, amount } => {
                if let Some(d) = self.decks.get_mut(deck.index()) {
                    d.set_pitch_bend(amount);
                }
            }
            C::SetTrim { deck, gain } => {
                if let Some(s) = self.strips.get_mut(deck.index())
                    && gain.0.is_finite()
                {
                    s.trim = gain.0.max(0.0);
                }
            }
            C::SetChannelFader { deck, position } => {
                if let Some(s) = self.strips.get_mut(deck.index())
                    && position.is_finite()
                {
                    s.fader = position.clamp(0.0, 1.0);
                }
            }
            C::SetMasterLevel { gain } => self.master_gain.set(gain.0),
            // TODO(M2): jog scratch and nudge, sync, loops, hot cues, beat jump,
            // slip, quantize, EQ, filter, crossfader, PFL and headphone mix.
            // TODO(M5): FX. TODO(M7): sampler, key shift.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_reaches_target_and_ignores_nan() {
        let mut r = Ramp::new(1.0);
        r.set(f32::NAN);
        assert_eq!(r.advance(4), (1.0, 0.0));
        r.set(0.0);
        let (start, step) = r.advance(4);
        assert_eq!(start, 1.0);
        assert!((start + step * 4.0).abs() < 1e-6);
    }
}
