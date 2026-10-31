//! The sampler: [`SAMPLER_SLOTS`] slots, each holding a sample, mixed into a
//! sampler bus that feeds the master (and, with PFL, the headphone cue bus).
//!
//! Everything here runs on the audio thread and never allocates after
//! [`Sampler::new`]. Samples arrive as [`TrackBuffer`]s over the same
//! hand-off ring as decks and leave on the same return ring.
//!
//! # Signal flow
//!
//! ```text
//! slot voice -> slot gain -+
//! slot voice -> slot gain -+-> bus -> sampler level -> master (before the master FX unit)
//!  ...                     '                       '-> cue bus when PFL is on
//! ```
//!
//! # Behaviour
//!
//! - Modes ([`SamplerMode`]): one-shot plays to the end, gate plays while the
//!   pad is held, loop repeats until stopped. A press while the slot sounds
//!   restarts it from the start (FLX4 manual p.62, step 5). `SamplerStop`
//!   stops it (SHIFT + pad, p.62 step 6).
//! - Restarts and stops fade the old sound out over 2 ms (it keeps reading
//!   the same buffer, so it costs nothing). The new sound starts at full level
//!   on its first sample so a transient keeps its attack. Replacing the sample
//!   of a playing slot cuts it, since the old buffer leaves with the swap.
//! - Quantize (default on): a press waits for the next beat of the master
//!   deck (the sync leader, else the first playing deck with a grid) and
//!   starts on that frame. The beat is taken from the previous block's view
//!   advanced by that block's length. Without a playing deck with a grid the
//!   press is immediate. A gate release before the beat cancels the start; a
//!   gate pressed and released inside one block with an immediate start
//!   sounds for that block.
//! - Sync (per slot, default off): the slot plays at `master BPM / sample BPM`
//!   by varispeed (pitch follows), folded to the octave closest to 1 (x2 or
//!   x0.5 like deck sync) and clamped to `0.25..=4`. It needs a sample BPM
//!   (`EngineHandle::load_sampler`) and a master tempo, else it plays at 1.
//!   The rate follows tempo changes with a per-block ramp. Stretching with the
//!   key kept was not chosen: a start would have to wait for the stretcher
//!   latency (about 120 ms at 48 kHz) or prime it like a deck, which costs a
//!   burst of work per trigger and 16 pads can fire at once.
//! - Loops wrap with modular interpolation, so a sample that is cut at a
//!   zero crossing repeats without a click.

use opendeck_contracts::ids::SAMPLER_SLOTS;
use opendeck_contracts::sampler::{SamplerMode, SamplerSlotTelemetry};
use opendeck_contracts::telemetry::{EngineTelemetry, StereoMeter};
use opendeck_contracts::{Bpm, EngineCommand, SampleRate, SamplerSlotId};

use crate::deck::{Interpolation, catmull_rom, frame_at, interpolate};
use crate::fx::FxView;
use crate::meter::MeterAcc;
use crate::mixer::Ramp;
use crate::track::TrackBuffer;

/// Slowest and fastest rate a synced slot plays at.
const MIN_SYNC_RATE: f64 = 0.25;
const MAX_SYNC_RATE: f64 = 4.0;

/// A voice that is playing.
#[derive(Debug, Clone, Copy)]
struct Voice {
    pos: f64,
}

/// A voice fading out after a restart or stop.
#[derive(Debug, Clone, Copy)]
struct Fading {
    pos: f64,
    left: usize,
}

/// Playback state of one slot, separate from its buffer so the render loop
/// can borrow both.
#[derive(Debug, Clone, Copy)]
struct State {
    voice: Option<Voice>,
    fading: Option<Fading>,
    /// Frames from the start of the next block until a start.
    pending: Option<f64>,
    /// A gate release arrived while an immediate start was still queued:
    /// stop at the end of the block that starts it.
    release: bool,
    /// Rate of the last rendered frame.
    rate: f64,
}

impl State {
    const fn idle() -> Self {
        Self {
            voice: None,
            fading: None,
            pending: None,
            release: false,
            rate: 1.0,
        }
    }

    /// Moves a playing voice to the fade-out slot.
    fn fade_out(&mut self, declick: usize) {
        if let Some(v) = self.voice.take() {
            self.fading = Some(Fading {
                pos: v.pos,
                left: declick,
            });
        }
    }

    fn is_active(&self) -> bool {
        self.voice.is_some() || self.fading.is_some()
    }
}

struct Slot {
    track: Option<TrackBuffer>,
    /// Tempo of the sample, for sync.
    bpm: Option<f64>,
    gain: Ramp,
    mode: SamplerMode,
    sync: bool,
    state: State,
}

impl Slot {
    fn new() -> Self {
        Self {
            track: None,
            bpm: None,
            gain: Ramp::new(1.0),
            mode: SamplerMode::OneShot,
            sync: false,
            state: State::idle(),
        }
    }
}

/// Tempo and beat of the master deck as of the last rendered block.
#[derive(Debug, Clone, Copy)]
struct BeatRef {
    /// Beat position at the first frame of that block.
    beat: f64,
    /// Beats per frame.
    per_frame: f64,
    /// Length of that block.
    frames: usize,
}

/// What the app sees of a slot, same-thread (offline tools and tests).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SamplerSlotStatus {
    /// A sample is loaded.
    pub loaded: bool,
    /// A voice is sounding (a fade-out tail does not count).
    pub playing: bool,
    /// A quantized start is waiting for its beat.
    pub waiting: bool,
    /// Position in source frames, fractional; 0 when not playing.
    pub position: f64,
    /// Playback rate of the last frame (`1.0` = original speed).
    pub rate: f64,
}

pub(crate) struct Sampler {
    slots: Vec<Slot>,
    level: Ramp,
    quantize: bool,
    pfl: bool,
    sample_rate: f64,
    declick: usize,
    interpolation: Interpolation,
    bus: Vec<f32>,
    meter: MeterAcc,
    beat_ref: Option<BeatRef>,
    any_active: bool,
}

impl Sampler {
    /// Allocates the slots and the bus. Not real-time safe.
    pub(crate) fn new(
        sample_rate: SampleRate,
        max_block_frames: usize,
        interpolation: Interpolation,
    ) -> Self {
        let declick = ((sample_rate.as_f64() * 0.002) as usize).clamp(16, max_block_frames.max(16));
        Self {
            slots: (0..SAMPLER_SLOTS).map(|_| Slot::new()).collect(),
            level: Ramp::new(1.0),
            quantize: true,
            pfl: false,
            sample_rate: sample_rate.as_f64(),
            declick,
            interpolation,
            bus: vec![0.0; max_block_frames.max(1) * 2],
            meter: MeterAcc::default(),
            beat_ref: None,
            any_active: false,
        }
    }

    /// Puts `track` in `slot` and returns the previous one. A sound playing in
    /// the slot is cut. Gain, mode and sync stay.
    pub(crate) fn load(
        &mut self,
        slot: SamplerSlotId,
        track: TrackBuffer,
        bpm: Option<Bpm>,
    ) -> Option<TrackBuffer> {
        let s = self.slots.get_mut(slot.index())?;
        s.state = State::idle();
        s.bpm = bpm.map(|b| b.0).filter(|b| b.is_finite() && *b > 0.0);
        s.track.replace(track)
    }

    /// Empties `slot` and returns its sample.
    pub(crate) fn clear(&mut self, slot: SamplerSlotId) -> Option<TrackBuffer> {
        let s = self.slots.get_mut(slot.index())?;
        s.state = State::idle();
        s.bpm = None;
        s.track.take()
    }

    /// Applies a sampler command. Returns false for any other command.
    pub(crate) fn apply(&mut self, command: EngineCommand) -> bool {
        use EngineCommand as C;
        match command {
            C::SamplerTrigger { slot, pressed } => self.trigger(slot, pressed),
            C::SamplerStop { slot } => {
                let declick = self.declick;
                if let Some(s) = self.slots.get_mut(slot.index()) {
                    s.state.pending = None;
                    s.state.release = false;
                    s.state.fade_out(declick);
                }
            }
            C::SetSamplerLevel { gain } => self.level.set(gain.0),
            C::SetSamplerMode { slot, mode } => {
                if let Some(s) = self.slots.get_mut(slot.index()) {
                    s.mode = mode;
                }
            }
            C::SetSamplerGain { slot, gain } => {
                if let Some(s) = self.slots.get_mut(slot.index()) {
                    s.gain.set(gain.0);
                }
            }
            C::SetSamplerSync { slot, on } => {
                if let Some(s) = self.slots.get_mut(slot.index()) {
                    s.sync = on;
                }
            }
            C::SetSamplerQuantize { on } => self.quantize = on,
            C::SetSamplerPfl { on } => self.pfl = on,
            _ => return false,
        }
        true
    }

    fn trigger(&mut self, slot: SamplerSlotId, pressed: bool) {
        let delay = self.frames_to_next_beat();
        let declick = self.declick;
        let Some(s) = self.slots.get_mut(slot.index()) else {
            return;
        };
        if s.track.is_none() {
            return;
        }
        if pressed {
            // Zero delay still goes through `pending` so the start lands on a
            // frame boundary inside the next block like a quantized one.
            s.state.pending = Some(delay);
            s.state.release = false;
        } else if s.mode == SamplerMode::Gate {
            if matches!(s.state.pending, Some(d) if d <= 0.0) {
                // Tapped inside one block: let it sound for that block.
                s.state.release = true;
            } else {
                s.state.pending = None;
                s.state.fade_out(declick);
            }
        }
    }

    /// Frames from the start of the next block to the next beat of the
    /// master deck; 0 with quantize off or no beat reference.
    fn frames_to_next_beat(&self) -> f64 {
        let Some(r) = self.beat_ref.filter(|_| self.quantize) else {
            return 0.0;
        };
        let beat_now = r.beat + r.frames as f64 * r.per_frame;
        let to_beat = beat_now.ceil() - beat_now;
        let frames = to_beat / r.per_frame;
        // On the beat (to a fraction of a frame) means now.
        if frames.is_finite() && frames >= 0.5 {
            frames
        } else {
            0.0
        }
    }

    /// Rate for a synced slot against the master view.
    fn sync_rate(slot: &Slot, view: &FxView) -> f64 {
        let (true, Some(sample_bpm), Some(master), true) =
            (slot.sync, slot.bpm, view.bpm, view.playing)
        else {
            return 1.0;
        };
        let r = master.0 / sample_bpm;
        if !(r.is_finite() && r > 0.0) {
            return 1.0;
        }
        // Closest of r, 2r, r/2 to 1 on a log scale.
        let dist = |x: f64| x.ln().abs();
        let mut best = r;
        for c in [r * 2.0, r * 0.5] {
            if dist(c) < dist(best) {
                best = c;
            }
        }
        best.clamp(MIN_SYNC_RATE, MAX_SYNC_RATE)
    }

    /// Renders the bus for a block of `frames`. `master` is the tempo and
    /// phase source (sync leader, else first playing deck with a grid) as of
    /// the first frame of the block. The bus (interleaved stereo, after slot
    /// gains and the sampler level) is read with [`bus`](Self::bus).
    pub(crate) fn render(&mut self, frames: usize, master: &FxView) {
        let declick = self.declick;
        let interpolation = self.interpolation;
        let Some(bus) = self.bus.get_mut(..frames * 2) else {
            self.any_active = false;
            return;
        };
        bus.fill(0.0);
        let mut any = false;
        for slot in &mut self.slots {
            if slot.track.is_none() {
                continue;
            }
            let target = Self::sync_rate(slot, master);
            let looped = slot.mode == SamplerMode::Loop;
            let Slot {
                track, gain, state, ..
            } = slot;
            let Some(track) = track.as_ref() else {
                continue;
            };
            if !state.is_active() && state.pending.is_none() {
                state.rate = target;
                gain.advance(frames);
                continue;
            }
            render_slot(
                track.samples(),
                state,
                gain,
                bus,
                Params {
                    target,
                    looped,
                    interpolation,
                    declick,
                },
            );
            any |= state.is_active();
        }
        self.any_active = any;

        let (start, step) = self.level.advance(frames);
        let silent = !any && start == 0.0 && step == 0.0;
        if !silent {
            for (i, frame) in bus.chunks_exact_mut(2).enumerate() {
                let g = start + step * (i + 1) as f32;
                frame[0] *= g;
                frame[1] *= g;
            }
        }
        self.meter.add(bus);

        self.beat_ref = match (master.bpm, master.beat) {
            (Some(bpm), Some(beat)) if master.playing && bpm.0 > 0.0 => Some(BeatRef {
                beat: beat.0,
                per_frame: bpm.0 / 60.0 / self.sample_rate,
                frames,
            }),
            _ => None,
        };
    }

    /// The bus of the last rendered block of `frames`.
    pub(crate) fn bus(&self, frames: usize) -> &[f32] {
        self.bus.get(..frames * 2).unwrap_or(&[])
    }

    /// True when the bus goes to the headphone cue bus as well.
    pub(crate) fn pfl(&self) -> bool {
        self.pfl
    }

    /// True when the last rendered block had a sound in it.
    pub(crate) fn is_active(&self) -> bool {
        self.any_active
    }

    pub(crate) fn take_meter(&mut self) -> StereoMeter {
        self.meter.take()
    }

    /// Writes slot state into a telemetry snapshot.
    pub(crate) fn fill_telemetry(&self, snapshot: &mut EngineTelemetry) {
        for (out, slot) in snapshot.sampler.iter_mut().zip(&self.slots) {
            let len = slot.track.as_ref().map_or(0.0, |t| t.frames() as f64);
            *out = SamplerSlotTelemetry {
                loaded: slot.track.is_some(),
                playing: slot.state.voice.is_some(),
                progress: slot.state.voice.map_or(0.0, |v| {
                    if len > 0.0 {
                        (v.pos / len).clamp(0.0, 1.0) as f32
                    } else {
                        0.0
                    }
                }),
            };
        }
    }

    /// State of one slot as of the last rendered block.
    pub(crate) fn status(&self, slot: SamplerSlotId) -> SamplerSlotStatus {
        self.slots
            .get(slot.index())
            .map_or_else(SamplerSlotStatus::default, |s| SamplerSlotStatus {
                loaded: s.track.is_some(),
                playing: s.state.voice.is_some(),
                waiting: s.state.pending.is_some(),
                position: s.state.voice.map_or(0.0, |v| v.pos),
                rate: s.state.rate,
            })
    }
}

#[derive(Debug, Clone, Copy)]
struct Params {
    target: f64,
    looped: bool,
    interpolation: Interpolation,
    declick: usize,
}

/// Renders one slot into `bus` (accumulating) for the whole block.
fn render_slot(data: &[f32], state: &mut State, gain: &mut Ramp, bus: &mut [f32], p: Params) {
    let n = bus.len() / 2;
    let r0 = state.rate;
    let r_step = (p.target - r0) / n.max(1) as f64;
    state.rate = p.target;
    let (g0, g_step) = gain.advance(n);
    let ramp = Ramps {
        r0,
        r_step,
        g0,
        g_step,
    };

    // A quantized start falls inside this block, or waits.
    let start_at = match state.pending {
        Some(d) if d < n as f64 => {
            state.pending = None;
            Some((d.round().max(0.0) as usize).min(n - 1))
        }
        Some(d) => {
            state.pending = Some(d - n as f64);
            None
        }
        None => None,
    };
    let split = start_at.unwrap_or(n);
    render_range(data, state, bus, 0..split, &ramp, p);
    if let Some(k) = start_at {
        state.fade_out(p.declick);
        state.voice = Some(Voice { pos: 0.0 });
        render_range(data, state, bus, k..n, &ramp, p);
        if state.release {
            state.release = false;
            state.fade_out(p.declick);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Ramps {
    r0: f64,
    r_step: f64,
    g0: f32,
    g_step: f32,
}

/// Renders frames `range` of the block: the voice and its fading predecessor.
fn render_range(
    data: &[f32],
    state: &mut State,
    bus: &mut [f32],
    range: std::ops::Range<usize>,
    ramps: &Ramps,
    p: Params,
) {
    let len = (data.len() / 2) as f64;
    let Some(window) = bus.get_mut(range.start * 2..range.end * 2) else {
        return;
    };
    for (j, frame) in window.chunks_exact_mut(2).enumerate() {
        let i = range.start + j;
        let t = (i + 1) as f64;
        let rate = ramps.r0 + ramps.r_step * t;
        let gain = ramps.g0 + ramps.g_step * t as f32;
        if let Some(v) = state.voice.as_mut() {
            let (l, r) = read(data, v.pos, p.interpolation, p.looped);
            frame[0] += l * gain;
            frame[1] += r * gain;
            v.pos += rate;
            if v.pos >= len {
                if p.looped && len > 0.0 {
                    v.pos %= len;
                } else {
                    state.voice = None;
                }
            }
        }
        if let Some(f) = state.fading.as_mut() {
            let fade = f.left as f32 / p.declick as f32;
            let (l, r) = read(data, f.pos, p.interpolation, p.looped);
            frame[0] += l * gain * fade;
            frame[1] += r * gain * fade;
            f.pos += rate;
            f.left -= 1;
            if f.left == 0 || (f.pos >= len && !p.looped) {
                state.fading = None;
            } else if f.pos >= len {
                f.pos %= len;
            }
        }
    }
}

/// Reads a stereo frame at `pos`; with `looped` the neighbours wrap around
/// the end of the sample.
#[inline]
fn read(data: &[f32], pos: f64, mode: Interpolation, looped: bool) -> (f32, f32) {
    if !looped {
        return interpolate(data, pos, mode);
    }
    let len = ((data.len() / 2) as i64).max(1);
    let base = pos.floor();
    let t = (pos - base) as f32;
    let i = base as i64;
    let at = |k: i64| frame_at(data, k.rem_euclid(len));
    let (l1, r1) = at(i);
    if t == 0.0 {
        return (l1, r1);
    }
    let (l2, r2) = at(i + 1);
    match mode {
        Interpolation::Linear => (l1 + (l2 - l1) * t, r1 + (r2 - r1) * t),
        Interpolation::Cubic => {
            let (l0, r0) = at(i - 1);
            let (l3, r3) = at(i + 2);
            (
                catmull_rom(l0, l1, l2, l3, t),
                catmull_rom(r0, r1, r2, r3, t),
            )
        }
    }
}
