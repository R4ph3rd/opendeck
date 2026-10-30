//! The FX host: units, slots, sends and returns (ADR 0006).
//!
//! # Topology
//!
//! There are `MAX_DECKS` deck units, one master unit and `FX_SEND_BUSES` send
//! units. Every unit is a serial chain of `FX_SLOTS_PER_UNIT` slots; slot 0
//! processes first.
//!
//! ```text
//! deck -> trim -> EQ -> filter -> [deck unit chain] -+-> PFL tap
//!                                                    +-> pre-fader send
//!                                                    '-> fader -> meter -> crossfader side -> master bus
//!                                                                          '-> post-fader send
//! send bus n -> [send unit n chain] -> return -> master bus
//! master bus -> [master unit chain] -> master level
//! ```
//!
//! Product choices (not external specs):
//!
//! - The deck insert sits after EQ and filter and before the PFL tap, so the
//!   headphone cue hears the effect. It is before the channel fader, so the
//!   fader also fades the tail of a delay or reverb.
//! - A send is tapped pre-fader (after the insert chain) or post-fader per
//!   bus ([`SendTap`], default post). "Post" is exactly what the channel adds
//!   to the master bus: after the channel fader and the crossfader side.
//! - Send returns are summed into the master bus before the master unit, so
//!   the master chain also processes the returns.
//! - The master unit takes tempo and phase from the sync leader (or the first
//!   playing deck with a grid); so do the send units.
//!
//! # Slot host
//!
//! The effect writes the fully wet signal. The host owns:
//!
//! - wet/dry: equal power (`dry = cos`, `wet = sin` of `wet * pi / 2`), linear
//!   ramp over the block. Note that equal power adds 3 dB when wet and dry are
//!   correlated and the mix is 50 %.
//! - bypass: a 10 ms linear crossfade between the mixed signal and the dry
//!   signal. An effect with a tail (`EffectInfo::has_tail`) is fed a faded
//!   out input instead and keeps running until its output stays under -90
//!   dBFS for 4096 frames (or [`MAX_TAIL_FRAMES`] pass), so a delay or reverb
//!   rings out after it is disabled.
//! - latency compensation: the dry path goes through a delay line of
//!   `latency_frames` as read once at install. The delay line stays in the
//!   path while an effect with latency is installed, even when bypassed, so
//!   enabling and disabling never shifts the timing. Limits: compensation is
//!   inside the slot only (a deck with a lookahead effect is late against the
//!   other deck by that latency) and capped at [`MAX_LATENCY_FRAMES`].
//!
//! In a send unit the dry path is silence while no slot is enabled, so a
//! disabled reverb does not return the channel signal on top of the channel.
//! Send slots should run at 100 % wet (the default); a lower mix returns that
//! share of the send input as well.
//!
//! # Beat FX units on a controller (DDJ-FLX4 style)
//!
//! - Unit to channel: a hardware "Beat FX 1" assigned to channel 1, 2 or 1&2
//!   maps to the `FxUnit::Deck` chains. Assigned to one channel it is that
//!   deck's chain. For "1&2" the app keeps one instance per channel, mirrors
//!   `SetFx*` commands to both chains and points both at the same tempo with
//!   [`FxTempoSource::Deck`], so the two instances run in lockstep. (One
//!   instance on the sum of the channels is not offered: it would need a
//!   summing unit the contracts do not have, and per channel instances keep
//!   each channel's own fader, EQ and cue.)
//! - Tempo source: [`FxTempoSource`] per unit. Auto is the unit's deck (the
//!   leader for master and send units), `Deck` forces a deck, `Tap` runs a
//!   private beat counter at a tapped BPM.
//! - Pad FX (momentary): `EngineHandle::schedule_fx_enabled` with `fast` on.
//!   The switch is applied at its frame inside the block, not at the block
//!   start, and ramps over 1 ms. An asleep slot takes its wet/dry at once, so
//!   the engage lands on the frame.
//! - Release FX: `EngineHandle::release_fx` bypasses one slot and runs a
//!   release slot for a hold time. The release slot's own bypass then lets its
//!   tail ring (the host keeps tail effects alive after disable).
//!
//! # Threads
//!
//! Effects are built, prepared and wrapped in a [`SlotRt`] (the effect, the
//! delay line and every scratch buffer) on the control thread. The box
//! reaches the audio thread over the FX hand-off ring and a replaced box
//! leaves on the FX return ring, so the audio thread never allocates or
//! frees. Parameters are applied to the effect as given (normalized, clamped
//! to `[0, 1]`, unknown ids ignored); smoothing inside the effect is its own
//! job (contract). Setting the initial parameters before the install is
//! allowed: `Effect::set_parameter` is real-time safe on any thread.
//!
//! A swap is a hard cut of the old effect. To avoid a click, disable the slot
//! and wait about 10 ms (or until its status is inactive) before installing.

use opendeck_contracts::fx::{Effect, ParamId, ProcessContext, StereoBufferMut};
use opendeck_contracts::ids::{FX_SEND_BUSES, FX_SLOTS_PER_UNIT};
use opendeck_contracts::{
    Beats, Bpm, DeckId, EngineCommand, Frames, FxSlotId, FxUnit, MAX_DECKS, SampleRate,
};

use crate::mixer::Ramp;

/// Bypass crossfade length, seconds.
const BYPASS_SECONDS: f32 = 0.010;

/// Fast ramp of a timed event (pad FX engage), seconds.
const FAST_SECONDS: f32 = 0.001;

/// Output level under which a tail counts as finished: -90 dBFS.
const TAIL_FLOOR: f32 = 3.162_277_7e-5;

/// Frames the output must stay under [`TAIL_FLOOR`] before a disabled slot
/// stops processing.
const TAIL_QUIET_FRAMES: u64 = 4096;

/// Longest a disabled effect is kept alive for its tail (10 s at 48 kHz).
pub const MAX_TAIL_FRAMES: u64 = 480_000;

/// Largest delay compensation, in frames (about 340 ms at 48 kHz).
pub const MAX_LATENCY_FRAMES: usize = 16_384;

/// Highest linear send level (+6 dB). Product choice.
const MAX_SEND_LEVEL: f32 = 2.0;

/// Beat division limits, beats. Product choice.
const MIN_DIVISION: f64 = 1.0 / 64.0;
const MAX_DIVISION: f64 = 64.0;

/// Number of units: decks, master, send buses.
pub(crate) const FX_UNITS: usize = MAX_DECKS + 1 + FX_SEND_BUSES;

const MASTER_UNIT: usize = MAX_DECKS;
const FIRST_SEND_UNIT: usize = MAX_DECKS + 1;

/// Where a send bus takes its signal from the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendTap {
    /// After the insert chain, before the channel fader.
    PreFader,
    /// After the channel fader and crossfader side (what reaches the master).
    #[default]
    PostFader,
}

/// Where a unit takes tempo and beat phase from (the BPM source of the
/// DDJ-FLX4 Beat FX: Auto or Tap).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FxTempoSource {
    /// Deck units: their own deck. Master and send units: the sync leader (or
    /// the first playing deck with a grid).
    #[default]
    Auto,
    /// Always this deck, whatever channel the unit sits on. Two units pointed
    /// at the same deck see identical tempo and phase (the "1&2" assignment).
    Deck(DeckId),
    /// A tapped tempo. The unit runs its own beat counter at this BPM,
    /// starting at beat 0 when the source is set; the play flag still comes
    /// from the unit's usual source.
    Tap(Bpm),
}

/// Same-thread view of one slot, for tests and offline tools. The control
/// thread has no telemetry for FX yet (contract change needed).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FxSlotStatus {
    /// An effect is installed.
    pub installed: bool,
    /// `SetFxEnabled` is on.
    pub enabled: bool,
    /// The effect is being processed: enabled, fading, or ringing out its tail.
    pub active: bool,
    /// Peak of the effect's wet output over the last block it ran.
    pub level: f32,
    /// Latency the host compensates, frames.
    pub latency_frames: u32,
    /// Wet/dry target, `[0, 1]`.
    pub wet: f32,
    /// Beat division target, beats.
    pub beat_division: f64,
}

/// Same-thread view of one unit.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FxUnitStatus {
    /// At least one slot is enabled.
    pub enabled: bool,
    /// At least one slot is processing (enabled, fading or ringing).
    pub active: bool,
    /// Peak of the unit's output over the last block it ran.
    pub level: f32,
}

/// Tempo and phase for one block, as seen by a unit.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FxView {
    /// Tempo of the source (grid BPM times rate).
    pub(crate) bpm: Option<Bpm>,
    /// Beat position at the first frame of the block.
    pub(crate) beat: Option<Beats>,
    /// The source is playing.
    pub(crate) playing: bool,
}

/// Everything an installed effect needs, allocated off the audio thread.
pub(crate) struct SlotRt {
    effect: Box<dyn Effect>,
    has_tail: bool,
    latency: usize,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    delay_pos: usize,
    dry_l: Vec<f32>,
    dry_r: Vec<f32>,
    wet_l: Vec<f32>,
    wet_r: Vec<f32>,
    env: Vec<f32>,
}

impl SlotRt {
    /// Prepares `effect` for the engine and allocates its buffers. Control or
    /// loader thread only.
    pub(crate) fn new(
        mut effect: Box<dyn Effect>,
        sample_rate: SampleRate,
        max_block: usize,
    ) -> Self {
        let max_block = max_block.max(1);
        effect.prepare(sample_rate, max_block);
        let latency = (effect.latency_frames() as usize).min(MAX_LATENCY_FRAMES);
        let has_tail = effect.info().has_tail;
        Self {
            effect,
            has_tail,
            latency,
            delay_l: vec![0.0; latency],
            delay_r: vec![0.0; latency],
            delay_pos: 0,
            dry_l: vec![0.0; max_block],
            dry_r: vec![0.0; max_block],
            wet_l: vec![0.0; max_block],
            wet_r: vec![0.0; max_block],
            env: vec![0.0; max_block],
        }
    }

    /// Gives the effect back (hand-off ring was full).
    pub(crate) fn into_effect(self) -> Box<dyn Effect> {
        self.effect
    }

    /// Delays one stereo sample by the compensation latency.
    #[inline]
    fn delay(&mut self, l: f32, r: f32) -> (f32, f32) {
        let pos = self.delay_pos;
        match (self.delay_l.get_mut(pos), self.delay_r.get_mut(pos)) {
            (Some(dl), Some(dr)) => {
                let out = (*dl, *dr);
                *dl = l;
                *dr = r;
                self.delay_pos = if pos + 1 >= self.latency { 0 } else { pos + 1 };
                out
            }
            _ => (l, r),
        }
    }
}

/// Items on the FX hand-off ring (control -> audio).
pub(crate) enum FxHandoff {
    /// Put `rt` into a slot, replacing what is there.
    Install {
        unit: usize,
        slot: usize,
        rt: Box<SlotRt>,
    },
    /// Empty a slot.
    Clear { unit: usize, slot: usize },
    /// Change where a send bus takes its signal.
    SendTap { bus: u8, tap: SendTap },
    /// Change a unit's tempo source.
    Tempo { unit: usize, source: FxTempoSource },
    /// Queue a timed enable or disable.
    Event(FxEvent),
}

/// A timed enable or disable of one slot, in engine frames.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FxEvent {
    pub(crate) unit: usize,
    pub(crate) slot: usize,
    pub(crate) on: bool,
    /// Engine frame (`EngineTelemetry::engine_time` clock) it takes effect at.
    pub(crate) at: Frames,
    /// Short (1 ms) ramp instead of the 10 ms bypass crossfade.
    pub(crate) fast: bool,
}

/// Timed events held on the audio thread at once.
const MAX_EVENTS: usize = 16;

/// A switch inside the current block.
#[derive(Debug, Clone, Copy, Default)]
struct Flip {
    at: usize,
    on: bool,
    fast: bool,
}

/// Flips per slot per block.
const MAX_FLIPS: usize = 4;

/// One position in a chain.
struct Slot {
    rt: Option<Box<SlotRt>>,
    enabled: bool,
    wet: f32,
    mix_dry: f32,
    mix_wet: f32,
    division: f64,
    /// 0 = bypassed, 1 = engaged; ramps over [`BYPASS_SECONDS`].
    engaged: f32,
    /// The effect is being processed.
    awake: bool,
    quiet_frames: u64,
    tail_frames: u64,
    level: f32,
    /// Current ramp step per frame; a timed event may make it faster.
    step: f32,
    default_step: f32,
    fast_step: f32,
    /// `enabled` as of the last processed frame, to find engage edges.
    last_on: bool,
    flips: [Flip; MAX_FLIPS],
    nflips: usize,
}

impl Slot {
    fn new(wet: f32, step: f32, fast_step: f32) -> Self {
        let (mix_dry, mix_wet) = mix_gains(wet);
        Self {
            rt: None,
            enabled: false,
            wet,
            mix_dry,
            mix_wet,
            division: 1.0,
            engaged: 0.0,
            awake: false,
            quiet_frames: 0,
            tail_frames: 0,
            level: 0.0,
            step,
            default_step: step,
            fast_step,
            last_on: false,
            flips: [Flip::default(); MAX_FLIPS],
            nflips: 0,
        }
    }

    /// Adds a switch at frame `at` of the block, keeping them in time order.
    /// With no room the last state wins at once.
    fn push_flip(&mut self, flip: Flip) {
        if self.nflips >= MAX_FLIPS {
            self.enabled = flip.on;
            return;
        }
        let mut i = self.nflips;
        while i > 0 && self.flips.get(i - 1).is_some_and(|f| f.at > flip.at) {
            self.flips[i] = self.flips[i - 1];
            i -= 1;
        }
        self.flips[i] = flip;
        self.nflips += 1;
    }

    fn wakes_in_block(&self) -> bool {
        self.flips.iter().take(self.nflips).any(|f| f.on)
    }

    fn running(&self) -> bool {
        self.rt.is_some() && self.awake
    }

    /// True when this slot must run even though it is bypassed: it carries
    /// latency that has to stay in the path.
    fn delays(&self) -> bool {
        self.rt.as_ref().is_some_and(|rt| rt.latency > 0)
    }

    fn status(&self) -> FxSlotStatus {
        FxSlotStatus {
            installed: self.rt.is_some(),
            enabled: self.enabled,
            active: self.running(),
            level: self.level,
            latency_frames: self.rt.as_ref().map_or(0, |rt| rt.latency as u32),
            wet: self.wet,
            beat_division: self.division,
        }
    }

    /// Processes one block in place. `idle_dry` is the dry gain of a bypassed
    /// slot (1 in a normal chain, 0 in a send chain with nothing enabled).
    fn process(
        &mut self,
        l: &mut [f32],
        r: &mut [f32],
        view: &FxView,
        sample_rate: SampleRate,
        idle_dry: f32,
    ) {
        let n = l.len().min(r.len());
        let wakes = self.enabled || self.wakes_in_block();
        let Some(rt) = self.rt.as_deref_mut() else {
            return;
        };
        if !self.awake {
            if wakes {
                // Idle to running (the engage edge below resets the effect).
                // Asleep means nothing audible to keep continuous: take the
                // current wet/dry at once so an engage lands on its frame.
                (self.mix_dry, self.mix_wet) = mix_gains(self.wet);
                self.awake = true;
                self.quiet_frames = 0;
                self.tail_frames = 0;
            } else {
                // Bypassed and settled: only the delay line stays in the path.
                if rt.latency > 0 {
                    for i in 0..n {
                        if let (Some(a), Some(b)) = (l.get_mut(i), r.get_mut(i)) {
                            let (dl, dr) = rt.delay(*a, *b);
                            *a = dl;
                            *b = dr;
                        }
                    }
                }
                return;
            }
        }

        let (Some(dry_l), Some(dry_r), Some(wet_l), Some(wet_r), Some(env)) = (
            rt.dry_l.get_mut(..n),
            rt.dry_r.get_mut(..n),
            rt.wet_l.get_mut(..n),
            rt.wet_r.get_mut(..n),
            rt.env.get_mut(..n),
        ) else {
            return;
        };

        // Enable ramp per sample; timed events switch the target mid-block.
        let mut target = if self.enabled { 1.0 } else { 0.0 };
        let mut step = self.step;
        let mut e = self.engaged;
        let mut next = 0;
        // Frames where the slot goes from disabled to enabled: the effect is
        // reset there, on the frame, so capture style effects (roll, brake)
        // start with the engage.
        let mut edges = [0_usize; MAX_FLIPS + 1];
        let mut nedges = 0;
        let mut on = self.enabled;
        if self.enabled && !self.last_on {
            nedges = 1;
        }
        for (i, slot) in env.iter_mut().enumerate() {
            while let Some(f) = self
                .flips
                .get(next)
                .copied()
                .filter(|f| next < self.nflips && f.at <= i)
            {
                if f.on
                    && !on
                    && let Some(edge) = edges.get_mut(nedges)
                {
                    *edge = i;
                    nedges += 1;
                }
                on = f.on;
                self.enabled = f.on;
                target = if f.on { 1.0 } else { 0.0 };
                step = if f.fast {
                    self.fast_step
                } else {
                    self.default_step
                };
                next += 1;
            }
            e = if e < target {
                (e + step).min(target)
            } else {
                (e - step).max(target)
            };
            *slot = e;
        }
        self.step = step;
        self.engaged = e;
        self.nflips = 0;
        self.last_on = on;

        // Delayed dry path and the effect input.
        let latency = rt.latency;
        for i in 0..n {
            let (Some(il), Some(ir)) = (l.get(i).copied(), r.get(i).copied()) else {
                continue;
            };
            let (dl, dr) = if latency > 0 {
                // Inline of `SlotRt::delay` on the split borrows.
                let pos = rt.delay_pos;
                match (rt.delay_l.get_mut(pos), rt.delay_r.get_mut(pos)) {
                    (Some(a), Some(b)) => {
                        let out = (*a, *b);
                        *a = il;
                        *b = ir;
                        rt.delay_pos = if pos + 1 >= latency { 0 } else { pos + 1 };
                        out
                    }
                    _ => (il, ir),
                }
            } else {
                (il, ir)
            };
            let feed = if rt.has_tail {
                env.get(i).copied().unwrap_or(1.0)
            } else {
                1.0
            };
            if let (Some(a), Some(b), Some(c), Some(d)) = (
                dry_l.get_mut(i),
                dry_r.get_mut(i),
                wet_l.get_mut(i),
                wet_r.get_mut(i),
            ) {
                *a = dl;
                *b = dr;
                *c = il * feed;
                *d = ir * feed;
            }
        }

        let ctx = ProcessContext {
            sample_rate,
            bpm: view.bpm,
            beat_position: view.beat,
            beat_division: Beats(self.division),
            playing: view.playing,
        };
        // Run the effect in segments split at the engage edges, resetting it
        // at each. The beat position advances with the segment start.
        let beats_per_frame = view
            .bpm
            .map_or(0.0, |b| b.0 / 60.0 / f64::from(sample_rate.0.max(1)));
        let mut start = 0;
        let mut boundaries = edges.iter().take(nedges).copied().chain(Some(n)).peekable();
        while let Some(end) = boundaries.next() {
            if end > start {
                let seg = ProcessContext {
                    beat_position: view
                        .beat
                        .map(|b| Beats(b.0 + start as f64 * beats_per_frame)),
                    ..ctx
                };
                if let (Some(a), Some(b)) = (wet_l.get_mut(start..end), wet_r.get_mut(start..end))
                    && let Some(io) = StereoBufferMut::new(a, b)
                {
                    rt.effect.process(io, &seg);
                }
                start = end;
            }
            // Every boundary but the last is an engage edge.
            if boundaries.peek().is_some() {
                rt.effect.reset();
            }
        }

        // Mix: equal power, ramped over the block.
        let (cd1, cw1) = mix_gains(self.wet);
        let (cd0, cw0) = (self.mix_dry, self.mix_wet);
        self.mix_dry = cd1;
        self.mix_wet = cw1;
        let inv = 1.0 / n.max(1) as f32;
        let (dcd, dcw) = ((cd1 - cd0) * inv, (cw1 - cw0) * inv);
        let mut peak = 0.0_f32;
        for i in 0..n {
            let t = (i + 1) as f32;
            let (cd, cw) = (cd0 + dcd * t, cw0 + dcw * t);
            let e = env.get(i).copied().unwrap_or(0.0);
            let dry_gain = cd * e + idle_dry * (1.0 - e);
            let wet_gain = if rt.has_tail { cw } else { cw * e };
            let (wl, wr) = (
                wet_l.get(i).copied().unwrap_or(0.0),
                wet_r.get(i).copied().unwrap_or(0.0),
            );
            peak = peak.max(wl.abs()).max(wr.abs());
            if let (Some(a), Some(b), Some(dl), Some(dr)) =
                (l.get_mut(i), r.get_mut(i), dry_l.get(i), dry_r.get(i))
            {
                *a = guard(dl * dry_gain + wl * wet_gain);
                *b = guard(dr * dry_gain + wr * wet_gain);
            }
        }
        self.level = peak;

        // Sleep once nothing audible is left.
        if self.enabled || self.engaged > 0.0 {
            self.quiet_frames = 0;
            self.tail_frames = 0;
        } else if !rt.has_tail {
            self.awake = false;
        } else {
            self.tail_frames += n as u64;
            if peak < TAIL_FLOOR {
                self.quiet_frames += n as u64;
            } else {
                self.quiet_frames = 0;
            }
            if self.quiet_frames >= TAIL_QUIET_FRAMES || self.tail_frames >= MAX_TAIL_FRAMES {
                self.awake = false;
                self.level = 0.0;
            }
        }
    }
}

#[inline]
fn guard(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

/// Equal-power `(dry, wet)` gains for a wet amount in `[0, 1]`.
fn mix_gains(wet: f32) -> (f32, f32) {
    if wet >= 1.0 {
        (0.0, 1.0)
    } else if wet <= 0.0 {
        (1.0, 0.0)
    } else {
        let a = wet * std::f32::consts::FRAC_PI_2;
        (a.cos(), a.sin())
    }
}

/// A chain of slots.
struct Unit {
    slots: [Slot; FX_SLOTS_PER_UNIT],
    is_send: bool,
    level: f32,
    tempo: FxTempoSource,
    /// Beat counter of a tapped tempo, at the start of the next block.
    tap_beat: f64,
}

impl Unit {
    fn new(is_send: bool, step: f32, fast_step: f32) -> Self {
        let wet = if is_send { 1.0 } else { 0.5 };
        Self {
            slots: std::array::from_fn(|_| Slot::new(wet, step, fast_step)),
            is_send,
            level: 0.0,
            tempo: FxTempoSource::Auto,
            tap_beat: 0.0,
        }
    }

    /// The chain has work: a slot is processing or about to wake, or holds
    /// latency in the path (not in a send unit, whose idle chain returns
    /// nothing).
    fn needs_run(&self) -> bool {
        self.slots.iter().any(|s| {
            s.running()
                || ((s.enabled || s.wakes_in_block()) && s.rt.is_some())
                || (!self.is_send && s.delays())
        })
    }

    fn any_enabled(&self) -> bool {
        self.slots.iter().any(|s| s.enabled && s.rt.is_some())
    }

    fn status(&self) -> FxUnitStatus {
        FxUnitStatus {
            enabled: self.any_enabled(),
            active: self.slots.iter().any(Slot::running),
            level: self.level,
        }
    }
}

/// All FX state on the audio thread.
pub(crate) struct FxHost {
    sample_rate: SampleRate,
    units: [Unit; FX_UNITS],
    /// Tempo and phase each unit sees in the current block.
    unit_view: [FxView; FX_UNITS],
    /// Timed enables and disables not yet due.
    events: [Option<FxEvent>; MAX_EVENTS],
    send_level: [[Ramp; FX_SEND_BUSES]; MAX_DECKS],
    send_tap: [SendTap; FX_SEND_BUSES],
    /// Interleaved sum of the channel sends per bus.
    send_in: [Vec<f32>; FX_SEND_BUSES],
    /// What one channel adds to the master, for post-fader sends.
    tap: Vec<f32>,
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
}

impl FxHost {
    /// Allocates the scratch buffers. Not real-time safe.
    pub(crate) fn new(sample_rate: SampleRate, max_block: usize) -> Self {
        let max_block = max_block.max(1);
        let rate = sample_rate.0.max(1) as f32;
        let step = 1.0 / (BYPASS_SECONDS * rate).max(1.0);
        let fast_step = 1.0 / (FAST_SECONDS * rate).max(1.0);
        Self {
            sample_rate,
            units: std::array::from_fn(|i| Unit::new(i >= FIRST_SEND_UNIT, step, fast_step)),
            unit_view: [FxView::default(); FX_UNITS],
            events: [None; MAX_EVENTS],
            send_level: [[Ramp::new(0.0); FX_SEND_BUSES]; MAX_DECKS],
            send_tap: [SendTap::default(); FX_SEND_BUSES],
            send_in: std::array::from_fn(|_| vec![0.0; max_block * 2]),
            tap: vec![0.0; max_block * 2],
            scratch_l: vec![0.0; max_block],
            scratch_r: vec![0.0; max_block],
        }
    }

    pub(crate) fn unit_index(unit: FxUnit) -> usize {
        match unit {
            FxUnit::Deck(d) => d.index(),
            FxUnit::Master => MASTER_UNIT,
            FxUnit::Send(bus) => FIRST_SEND_UNIT + usize::from(bus).min(FX_SEND_BUSES - 1),
        }
    }

    fn slot_mut(&mut self, id: FxSlotId) -> Option<&mut Slot> {
        self.units
            .get_mut(Self::unit_index(id.unit()))
            .and_then(|u| u.slots.get_mut(id.slot()))
    }

    pub(crate) fn slot_status(&self, id: FxSlotId) -> Option<FxSlotStatus> {
        self.units
            .get(Self::unit_index(id.unit()))
            .and_then(|u| u.slots.get(id.slot()))
            .map(Slot::status)
    }

    pub(crate) fn unit_status(&self, unit: FxUnit) -> Option<FxUnitStatus> {
        self.units.get(Self::unit_index(unit)).map(Unit::status)
    }

    /// Applies one hand-off item. Returns the replaced box, if any.
    pub(crate) fn apply_handoff(&mut self, item: FxHandoff) -> Option<Box<SlotRt>> {
        match item {
            FxHandoff::Install { unit, slot, rt } => {
                let target = self.units.get_mut(unit).and_then(|u| u.slots.get_mut(slot));
                match target {
                    Some(s) => {
                        s.engaged = 0.0;
                        s.awake = false;
                        s.last_on = false;
                        s.level = 0.0;
                        s.rt.replace(rt)
                    }
                    // Unreachable (indices come from `FxSlotId`); give it back.
                    None => Some(rt),
                }
            }
            FxHandoff::Clear { unit, slot } => {
                let target = self.units.get_mut(unit).and_then(|u| u.slots.get_mut(slot));
                target.and_then(|s| {
                    s.engaged = 0.0;
                    s.awake = false;
                    s.level = 0.0;
                    s.rt.take()
                })
            }
            FxHandoff::SendTap { bus, tap } => {
                if let Some(t) = self.send_tap.get_mut(usize::from(bus)) {
                    *t = tap;
                }
                None
            }
            FxHandoff::Tempo { unit, source } => {
                if let Some(u) = self.units.get_mut(unit) {
                    u.tempo = source;
                    u.tap_beat = 0.0;
                }
                None
            }
            FxHandoff::Event(event) => {
                match self.events.iter_mut().find(|e| e.is_none()) {
                    Some(free) => *free = Some(event),
                    // Queue full: act now rather than lose the switch.
                    None => {
                        if let Some(s) = self
                            .units
                            .get_mut(event.unit)
                            .and_then(|u| u.slots.get_mut(event.slot))
                        {
                            s.enabled = event.on;
                        }
                    }
                }
                None
            }
        }
    }

    /// Applies an FX command. Returns false for any other command.
    pub(crate) fn apply(&mut self, command: EngineCommand) -> bool {
        use EngineCommand as C;
        match command {
            C::SetFxEnabled { slot, on } => {
                if let Some(s) = self.slot_mut(slot) {
                    s.enabled = on;
                    s.step = s.default_step;
                }
            }
            C::SetFxParam { slot, param, value } => {
                if value.is_finite()
                    && let Some(rt) = self.slot_mut(slot).and_then(|s| s.rt.as_deref_mut())
                    && has_param(rt.effect.as_ref(), param)
                {
                    rt.effect.set_parameter(param, value.clamp(0.0, 1.0));
                }
            }
            C::SetFxMix { slot, wet } => {
                if wet.is_finite()
                    && let Some(s) = self.slot_mut(slot)
                {
                    s.wet = wet.clamp(0.0, 1.0);
                }
            }
            C::SetFxBeatDivision { slot, beats } => {
                if beats.0.is_finite()
                    && beats.0 > 0.0
                    && let Some(s) = self.slot_mut(slot)
                {
                    s.division = beats.0.clamp(MIN_DIVISION, MAX_DIVISION);
                }
            }
            C::SetFxSend { deck, bus, level } => {
                if level.0.is_finite()
                    && let Some(r) = self
                        .send_level
                        .get_mut(deck.index())
                        .and_then(|b| b.get_mut(usize::from(bus)))
                {
                    r.set(level.0.min(MAX_SEND_LEVEL));
                }
            }
            _ => return false,
        }
        true
    }

    /// Prepares a block of `frames` starting at engine frame `block_start`:
    /// clears the send buses, resolves what tempo each unit sees and turns
    /// timed events that fall in the block into per-slot switches.
    /// `decks` are the deck views read before the decks move; `master` is the
    /// leader (or first playing) view.
    pub(crate) fn begin_block(
        &mut self,
        frames: usize,
        block_start: u64,
        decks: &[FxView; MAX_DECKS],
        master: &FxView,
    ) {
        for bus in &mut self.send_in {
            if let Some(b) = bus.get_mut(..frames * 2) {
                b.fill(0.0);
            }
        }
        let rate = f64::from(self.sample_rate.0.max(1));
        for (index, (unit, view)) in self
            .units
            .iter_mut()
            .zip(self.unit_view.iter_mut())
            .enumerate()
        {
            let own = decks.get(index).unwrap_or(master);
            let base = match unit.tempo {
                FxTempoSource::Auto | FxTempoSource::Tap(_) => *own,
                FxTempoSource::Deck(d) => decks.get(d.index()).copied().unwrap_or_default(),
            };
            *view = match unit.tempo {
                FxTempoSource::Tap(bpm) => {
                    let at = unit.tap_beat;
                    unit.tap_beat += bpm.0 / 60.0 * frames as f64 / rate;
                    FxView {
                        bpm: Some(bpm),
                        beat: Some(Beats(at)),
                        playing: base.playing,
                    }
                }
                _ => base,
            };
            for slot in &mut unit.slots {
                slot.nflips = 0;
            }
        }
        let block_end = block_start.saturating_add(frames as u64);
        for entry in &mut self.events {
            let Some(event) = *entry else {
                continue;
            };
            if event.at.0 >= block_end {
                continue;
            }
            *entry = None;
            let Some(slot) = self
                .units
                .get_mut(event.unit)
                .and_then(|u| u.slots.get_mut(event.slot))
            else {
                continue;
            };
            if slot.rt.is_none() {
                slot.enabled = event.on;
                continue;
            }
            // Late events start at the first frame of the block.
            let at = event.at.0.saturating_sub(block_start) as usize;
            slot.push_flip(Flip {
                at,
                on: event.on,
                fast: event.fast,
            });
        }
    }

    /// Runs the insert chain of deck `index` over `buf` (interleaved).
    pub(crate) fn insert_deck(&mut self, index: usize, buf: &mut [f32]) {
        self.run_unit(index, buf);
    }

    /// Pre-fader sends of channel `deck`: adds `buf` (after the insert chain).
    pub(crate) fn send_pre(&mut self, deck: usize, buf: &[f32]) {
        let frames = buf.len() / 2;
        for bus in 0..FX_SEND_BUSES {
            if self.send_tap.get(bus) == Some(&SendTap::PreFader) {
                self.accumulate(deck, bus, buf, frames);
            }
        }
    }

    /// Scratch for the post-fader tap of channel `deck`, or `None` when no
    /// post-fader send of this channel is open.
    pub(crate) fn post_tap(&mut self, deck: usize, frames: usize) -> Option<&mut [f32]> {
        let wanted = (0..FX_SEND_BUSES).any(|bus| {
            self.send_tap.get(bus) == Some(&SendTap::PostFader)
                && self
                    .send_level
                    .get(deck)
                    .and_then(|b| b.get(bus))
                    .is_some_and(|r| !r.is_at(0.0))
        });
        if wanted {
            self.tap.get_mut(..frames * 2)
        } else {
            None
        }
    }

    /// Adds the post-fader tap filled by the strip to the post-fader buses.
    pub(crate) fn send_post(&mut self, deck: usize, frames: usize) {
        let tap = std::mem::take(&mut self.tap);
        for bus in 0..FX_SEND_BUSES {
            if self.send_tap.get(bus) == Some(&SendTap::PostFader)
                && let Some(t) = tap.get(..frames * 2)
            {
                self.accumulate(deck, bus, t, frames);
            }
        }
        self.tap = tap;
    }

    fn accumulate(&mut self, deck: usize, bus: usize, src: &[f32], frames: usize) {
        let (Some(ramp), Some(acc)) = (
            self.send_level.get_mut(deck).and_then(|b| b.get_mut(bus)),
            self.send_in
                .get_mut(bus)
                .and_then(|b| b.get_mut(..frames * 2)),
        ) else {
            return;
        };
        if ramp.is_at(0.0) {
            return;
        }
        let (start, step) = ramp.advance(frames);
        for (i, (a, s)) in acc.chunks_exact_mut(2).zip(src.chunks_exact(2)).enumerate() {
            let g = start + step * (i + 1) as f32;
            a[0] += s[0] * g;
            a[1] += s[1] * g;
        }
    }

    /// Runs every send unit over its summed input and adds the return to
    /// `master`.
    pub(crate) fn process_sends(&mut self, master: &mut [f32]) {
        let frames = master.len() / 2;
        for bus in 0..FX_SEND_BUSES {
            let unit = FIRST_SEND_UNIT + bus;
            if !self.units.get(unit).is_some_and(Unit::needs_run) {
                if let Some(u) = self.units.get_mut(unit) {
                    u.level = 0.0;
                }
                continue;
            }
            let Some(mut input) = self.send_in.get_mut(bus).map(std::mem::take) else {
                continue;
            };
            if let Some(buf) = input.get_mut(..frames * 2) {
                self.run_unit(unit, buf);
                for (m, s) in master.iter_mut().zip(buf.iter()) {
                    *m += *s;
                }
            }
            if let Some(slot) = self.send_in.get_mut(bus) {
                *slot = input;
            }
        }
    }

    /// Runs the master unit over `master` (before the master level).
    pub(crate) fn process_master(&mut self, master: &mut [f32]) {
        self.run_unit(MASTER_UNIT, master);
    }

    fn run_unit(&mut self, index: usize, buf: &mut [f32]) {
        let frames = buf.len() / 2;
        let view = self.unit_view.get(index).copied().unwrap_or_default();
        let Some(unit) = self.units.get_mut(index) else {
            return;
        };
        if !unit.needs_run() {
            unit.level = 0.0;
            return;
        }
        let (Some(l), Some(r)) = (
            self.scratch_l.get_mut(..frames),
            self.scratch_r.get_mut(..frames),
        ) else {
            return;
        };
        for ((f, a), b) in buf.chunks_exact(2).zip(l.iter_mut()).zip(r.iter_mut()) {
            *a = f[0];
            *b = f[1];
        }
        let idle_dry = if unit.is_send && !unit.any_enabled() {
            0.0
        } else {
            1.0
        };
        for slot in &mut unit.slots {
            slot.process(l, r, &view, self.sample_rate, idle_dry);
        }
        let mut peak = 0.0_f32;
        for ((f, a), b) in buf.chunks_exact_mut(2).zip(l.iter()).zip(r.iter()) {
            f[0] = *a;
            f[1] = *b;
            peak = peak.max(a.abs()).max(b.abs());
        }
        unit.level = peak;
    }
}

/// True when `effect` declares parameter `id`.
fn has_param(effect: &dyn Effect, id: ParamId) -> bool {
    effect.info().params.iter().any(|p| p.id == id)
}
