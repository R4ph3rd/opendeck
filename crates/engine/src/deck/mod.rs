//! One deck: transport, CDJ-style CUE, tempo, pitch bend, key lock, loops, hot
//! cues, slip, jog and the deck side of sync.
//!
//! Everything in here runs on the audio thread. No allocation after
//! [`Deck::new`], no locks, no panics: indexing goes through `get`, float
//! to integer casts saturate, positions are clamped.
//!
//! # Playback model
//!
//! The playhead `pos` is a fractional source frame: the position of the audio
//! being heard now. Without key lock the output is read straight from the
//! track with linear or cubic interpolation (plain varispeed, pitch follows
//! tempo). With key lock the track feeds a Signalsmith stretcher whose input
//! `head` runs ahead of `pos` by the stretcher latency; `pos` itself advances
//! by exactly the playback rate per output frame, so telemetry is the same in
//! both modes.
//!
//! # Key lock start
//!
//! The stretcher delays its output by `input_latency / rate + output_latency`
//! frames when fed at the playback rate, so aligning it with `pos` the direct
//! way costs tens of thousands of frames at low rates: far too much for one
//! callback. Instead the stretcher is seeded so that it reaches alignment a
//! fixed number of output frames after the start (`warm`), and it runs
//! `PRIME_SPEEDUP` times faster than real time until then: at most
//! `PRIME_SPEEDUP * block` extra stretcher frames per callback, whatever the
//! rate. Until `warm` frames have played the deck outputs plain varispeed
//! (same position, pitch follows tempo); then it crossfades to the stretcher
//! over `PRIME_XFADE_SECONDS`. At rate 1.0 both are at the same pitch and the
//! swap is hard to hear; at other rates the first ~30 ms are varispeed.
//!
//! # C++ heap use
//!
//! The stretcher is C++, so `assert_no_alloc` cannot see its allocations.
//! `tests_heap` counts `malloc` and `free` calls from every language (glibc
//! only) and requires zero during key-locked playback. The stretcher's
//! scratch vectors are sized in `configure`; `process`, `seek` and `reset`
//! stay inside that capacity. Its peak list is reserved at `bands / 2`, enough
//! because `bands` is always even (see `tests_heap`); that list is only used
//! when transposing, which this deck never does. Key shift (M7) must keep
//! `tests_heap::transposed_stretcher_makes_no_heap_calls` green.
//!
//! # Click-free jumps
//!
//! Stopping, seeking, cue jumps and key lock changes would click if the
//! output cut abruptly. Before such a change the deck renders a short tail of
//! the old state with a fade-out and mixes it over the first frames of the
//! new state, which fades in. Both last `declick` frames (about 2 ms).
//!
//! Quantized jumps use the same mechanism in the middle of a block: the render
//! stops on the frame that crosses the trigger position, the tail continues
//! the old stream past it, and the new stream starts at the landing position
//! plus the sub-frame overshoot.
//!
//! Loop wraps need no tail. Over the last `declick` frames before the loop end
//! the output blends into the audio that precedes the loop start, then the
//! playhead steps back by the loop length (keeping its sub-frame offset). The
//! output is continuous across the wrap, the loop start keeps its transient at
//! full level, and for material that repeats at the loop length (a
//! beat-aligned loop of a steady beat) the blend is transparent, so the loop
//! is sample exact. With key lock the stretcher input is read through the loop
//! with the same seam blend, so the stretcher never sees a jump and does not
//! need to re-prime.
//!
//! # Slip
//!
//! `slip_pos` is a shadow playhead that keeps moving at the natural tempo
//! rate while the audible playhead scratches, loops or sits on a hot cue.
//! `slip_hold` records why it exists; when the last reason ends the playhead
//! jumps to the shadow. Loop roll always uses it; the other reasons only when
//! slip mode is on.

use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::FrameRegion;
use opendeck_contracts::telemetry::DeckTelemetry;
use opendeck_contracts::tempo::{SyncMode, TempoRange};
use opendeck_contracts::{Bpm, DeckId, EngineEvent, Frames, SampleRate, Seconds};
use rtrb::Producer;
use signalsmith_stretch::Stretch;

use crate::track::TrackBuffer;

mod cues;
mod jog;
mod loops;
mod sync;

pub use sync::SyncStatus;

/// Largest playback rate the deck renders, as a ratio of original speed.
pub const MAX_RATE: f64 = 4.0;

/// Pitch bend at full deflection, as a fraction of speed (`0.08` = +/- 8 %).
// TODO(verify): product tuning, open question in CONTEXT/engine.md.
pub const MAX_PITCH_BEND: f64 = 0.08;

/// Below this rate the key-locked output is silent and the playhead holds.
const KEY_LOCK_MIN_RATE: f64 = 0.05;

/// Output frames processed per step while priming the stretcher.
const PRIME_CHUNK: usize = 512;

/// The stretcher warms up this many times faster than real time, so a key
/// locked start is heard through the stretcher after about 30 ms. The extra
/// work per block is `PRIME_SPEEDUP` times the block, whatever the rate.
const PRIME_SPEEDUP: usize = 4;

/// Length of the crossfade from varispeed to the stretcher, in seconds.
const PRIME_XFADE_SECONDS: f64 = 0.010;

/// Trigger positions reached in one block, at most. A guard against loops so
/// short that a block would never finish.
const MAX_HITS_PER_BLOCK: usize = 16;

/// Slip reasons (bits of `Deck::slip_hold`).
const SLIP_SCRATCH: u8 = 1;
const SLIP_LOOP: u8 = 2;
const SLIP_ROLL: u8 = 4;
const SLIP_HOTCUE: u8 = 8;

/// Sample interpolation used by plain varispeed playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interpolation {
    /// Two-point linear interpolation. Cheapest, dulls highs at odd rates.
    Linear,
    /// Four-point Catmull-Rom cubic. Default.
    #[default]
    Cubic,
}

/// Where the deck is in its transport cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transport {
    Stopped,
    Playing,
    /// CUE or a hot cue held while stopped: plays until release.
    CuePreview,
}

/// A deck's state as of the last rendered block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckStatus {
    /// A track is loaded.
    pub loaded: bool,
    /// Playing, including CUE preview.
    pub playing: bool,
    /// Playhead in source frames, fractional.
    pub position: f64,
    /// Playback rate of the last rendered frame (`1.0` = original speed).
    pub rate: f64,
    /// Active loop `(start, end)` in source frames.
    pub active_loop: Option<(f64, f64)>,
    /// Where playback would be without slip, while slip is engaged.
    pub slip_position: Option<f64>,
    /// The platter is held in vinyl mode (scratching).
    pub scratching: bool,
    /// What sync is doing for this deck.
    pub sync: SyncStatus,
}

/// Sends events to the control thread without blocking and counts drops.
pub(crate) struct EventSink {
    pub(crate) producer: Producer<EngineEvent>,
    pub(crate) dropped: u32,
}

impl EventSink {
    pub(crate) fn push(&mut self, event: EngineEvent) {
        if self.producer.push(event).is_err() {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}

/// Progress of the key lock start (see the module docs).
#[derive(Debug, Clone, Copy)]
struct Priming {
    /// Output frames rendered since the start.
    done: usize,
    /// Output frames until the stretcher takes over.
    warm: usize,
    /// Length of the crossfade after `warm`.
    xfade: usize,
    /// Stretcher frames still to discard.
    work_left: usize,
    /// Playback rate the stretcher was seeded for.
    rate: f64,
}

/// A loop in source frames.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Loop {
    start: f64,
    end: f64,
}

impl Loop {
    fn len(&self) -> f64 {
        self.end - self.start
    }
}

/// A jump scheduled for a position (quantized cue, hot cue).
#[derive(Debug, Clone, Copy)]
struct Pending {
    at: f64,
    to: f64,
}

/// The next scheduled jump: at position `at`, land on `to`.
#[derive(Debug, Clone, Copy)]
struct Plan {
    at: f64,
    to: f64,
}

/// Playback rate for the frames of one render call: frame `i` advances the
/// playhead by `start + step * (i + 1)`.
#[derive(Debug, Clone, Copy)]
struct RateRamp {
    start: f64,
    step: f64,
}

impl RateRamp {
    fn constant(rate: f64) -> Self {
        Self {
            start: rate,
            step: 0.0,
        }
    }

    /// Mean rate over `n` frames.
    fn mean(&self, n: usize) -> f64 {
        self.start + self.step * (n as f64 + 1.0) * 0.5
    }
}

/// Options of one varispeed render.
#[derive(Debug, Clone, Copy)]
struct Varispeed {
    /// End the render on the frame whose advance reaches this position.
    stop_at: Option<f64>,
    /// Scratch mode: signed rate, clamped playhead, level follows speed.
    scratch: bool,
    /// Wrap at the end of this loop.
    looping: Option<Loop>,
}

/// Result of rendering part of a block.
#[derive(Debug, Clone, Copy)]
struct Rendered {
    frames: usize,
    hit: bool,
    ended: bool,
}

/// Stores a loop for the stretcher input: seam positions in source frames
/// and the crossfade length.
#[derive(Debug, Clone, Copy)]
struct InputLoop {
    start: i64,
    end: i64,
    xfade: usize,
}

/// One playback deck.
pub(crate) struct Deck {
    id: DeckId,
    sample_rate: f64,
    track: Option<TrackBuffer>,
    grid: Option<BeatGrid>,
    transport: Transport,
    /// Audible playhead, fractional source frames.
    pos: f64,
    /// Main cue point, source frames.
    cue: f64,
    /// Where a CUE or hot cue preview returns on release.
    preview_return: f64,
    preview_hot: Option<usize>,

    fader: f32,
    range: TempoRange,
    key_lock: bool,
    bend: f32,
    /// Rate used for the last rendered frame; ramps to the target per block.
    rate_cur: f64,
    interpolation: Interpolation,

    quantize: bool,
    vinyl: bool,
    slip: bool,
    slip_pos: Option<f64>,
    slip_hold: u8,

    loop_stored: Option<Loop>,
    loop_active: bool,
    loop_in: Option<f64>,
    roll: Option<RollSaved>,
    pending: Option<Pending>,
    hot: [Option<FrameRegion>; opendeck_contracts::HOT_CUES_PER_DECK],

    touched: bool,
    scratch: Option<jog::Scratch>,
    jog_owed: f64,
    jog_revs: f64,
    nudge: f64,

    sync_mode: SyncMode,
    sync_status: SyncStatus,
    held_rate: Option<f64>,
    phase_trim: f64,
    needs_align: bool,

    stretch: Stretch,
    /// Stretcher input position, source frames (key lock only).
    head: f64,
    input_latency: usize,
    output_latency: usize,
    scratch_in: Vec<f32>,
    scratch_out: Vec<f32>,
    /// Varispeed block rendered beside the stretcher while priming.
    scratch_v: Vec<f32>,
    /// `Some` from a key lock start until the stretcher has fully taken over.
    priming: Option<Priming>,
    xfade: usize,

    declick: usize,
    fade_in_left: usize,
    tail: Vec<f32>,
    tail_len: usize,
    tail_pos: usize,
    /// Frame of the current block where the tail starts playing.
    tail_at: usize,
}

/// Loop state to restore when a loop roll ends.
#[derive(Debug, Clone, Copy)]
struct RollSaved {
    stored: Option<Loop>,
    active: bool,
}

impl Deck {
    /// Allocates a deck. Not real-time safe: call before the stream starts.
    pub(crate) fn new(
        id: DeckId,
        sample_rate: SampleRate,
        max_block_frames: usize,
        interpolation: Interpolation,
    ) -> Self {
        let stretch = Stretch::preset_default(2, sample_rate.0);
        let input_latency = stretch.input_latency();
        let output_latency = stretch.output_latency();
        let declick = ((sample_rate.as_f64() * 0.002) as usize).clamp(16, max_block_frames.max(16));
        let scratch_frames = (max_block_frames.max(PRIME_CHUNK) * 4 + 16).max(input_latency + 16);
        Self {
            id,
            sample_rate: sample_rate.as_f64(),
            track: None,
            grid: None,
            transport: Transport::Stopped,
            pos: 0.0,
            cue: 0.0,
            preview_return: 0.0,
            preview_hot: None,
            fader: 0.0,
            range: TempoRange::default(),
            key_lock: false,
            bend: 0.0,
            rate_cur: 1.0,
            interpolation,
            quantize: true,
            vinyl: true,
            slip: false,
            slip_pos: None,
            slip_hold: 0,
            loop_stored: None,
            loop_active: false,
            loop_in: None,
            roll: None,
            pending: None,
            hot: [None; opendeck_contracts::HOT_CUES_PER_DECK],
            touched: false,
            scratch: None,
            jog_owed: 0.0,
            jog_revs: 0.0,
            nudge: 0.0,
            sync_mode: SyncMode::Off,
            sync_status: SyncStatus::Off,
            held_rate: None,
            phase_trim: 0.0,
            needs_align: false,
            stretch,
            head: 0.0,
            input_latency,
            output_latency,
            scratch_in: vec![0.0; scratch_frames * 2],
            scratch_out: vec![0.0; PRIME_CHUNK * 2],
            scratch_v: vec![0.0; max_block_frames.max(16) * 2],
            priming: None,
            xfade: ((sample_rate.as_f64() * PRIME_XFADE_SECONDS) as usize).max(1),
            declick,
            fade_in_left: 0,
            tail: vec![0.0; declick * 2],
            tail_len: 0,
            tail_pos: 0,
            tail_at: 0,
        }
    }

    fn len_frames(&self) -> f64 {
        self.track.as_ref().map_or(0.0, |t| t.frames() as f64)
    }

    fn audible(&self) -> bool {
        self.track.is_some() && (self.transport != Transport::Stopped || self.scratch.is_some())
    }

    /// Tempo rate from the fader, or the held sync rate.
    fn base_rate(&self) -> f64 {
        self.held_rate
            .unwrap_or_else(|| self.range.ratio(self.fader))
    }

    /// Rate the track would play at without scratching: tempo, sync, bend,
    /// jog nudge and phase correction.
    pub(crate) fn tempo_rate(&self) -> f64 {
        (self.base_rate()
            * (1.0 + f64::from(self.bend) * MAX_PITCH_BEND)
            * (1.0 + self.nudge)
            * (1.0 + self.phase_trim))
            .clamp(0.0, MAX_RATE)
    }

    fn target_rate(&self) -> f64 {
        match &self.scratch {
            Some(s) => s.rate,
            None => self.tempo_rate(),
        }
    }

    // ----- Loading -----

    /// Replaces the track. Returns the previous buffer for deferred free.
    /// Everything that belongs to the old track (loops, hot cues, slip) is
    /// cleared; settings (tempo, key lock, quantize, slip mode, sync) stay.
    pub(crate) fn load(
        &mut self,
        track: TrackBuffer,
        cue_point: Option<Frames>,
        grid: Option<BeatGrid>,
    ) -> Option<TrackBuffer> {
        let frames = track.frames() as f64;
        let old = self.track.replace(track);
        self.reset_track_state();
        self.grid = grid;
        self.cue = cue_point.map_or(0.0, |f| (f.0 as f64).min(frames));
        self.rate_cur = self.target_rate();
        old
    }

    /// Unloads the track and returns it for deferred free.
    pub(crate) fn eject(&mut self) -> Option<TrackBuffer> {
        self.reset_track_state();
        self.grid = None;
        self.cue = 0.0;
        self.track.take()
    }

    /// Replaces the beat grid of the loaded track.
    pub(crate) fn set_grid(&mut self, grid: Option<BeatGrid>) {
        self.grid = grid;
        if grid.is_none() {
            self.loop_in = None;
        }
    }

    fn reset_track_state(&mut self) {
        self.transport = Transport::Stopped;
        self.pos = 0.0;
        self.tail_len = 0;
        self.tail_pos = 0;
        self.tail_at = 0;
        self.fade_in_left = 0;
        self.priming = None;
        self.loop_stored = None;
        self.loop_active = false;
        self.loop_in = None;
        self.roll = None;
        self.pending = None;
        self.hot = [None; opendeck_contracts::HOT_CUES_PER_DECK];
        self.slip_pos = None;
        self.slip_hold = 0;
        self.scratch = None;
        self.jog_owed = 0.0;
        self.preview_hot = None;
        self.preview_return = 0.0;
        self.needs_align = false;
    }

    // ----- Transport commands -----

    pub(crate) fn set_playing(&mut self, playing: bool) {
        if self.track.is_none() {
            return;
        }
        match (playing, self.transport) {
            (true, Transport::Stopped) => {
                if self.pos >= self.len_frames() {
                    return;
                }
                self.transport = Transport::Playing;
                self.begin_audible();
                self.mark_align();
            }
            // PLAY while CUE is held latches playback.
            (true, Transport::CuePreview) => {
                self.transport = Transport::Playing;
                self.preview_hot = None;
            }
            (false, Transport::Playing | Transport::CuePreview) => {
                self.start_tail();
                self.transport = Transport::Stopped;
                self.on_stop();
            }
            _ => {}
        }
    }

    /// CUE with CDJ behaviour.
    ///
    /// - Playing, press: return to the cue point and pause.
    /// - Paused off the cue point, press: set the cue point at the playhead
    ///   (snapped to the beat grid when quantize is on).
    /// - Paused on the cue point, press: preview from the cue point.
    /// - Preview, release: return to the cue point and pause.
    pub(crate) fn cue(&mut self, pressed: bool, sink: &mut EventSink) {
        if self.track.is_none() {
            return;
        }
        match (pressed, self.transport) {
            (true, Transport::Playing) => self.stop_at(self.cue),
            (true, Transport::Stopped) => {
                if (self.pos - self.cue).abs() < 1.0 {
                    self.transport = Transport::CuePreview;
                    self.preview_return = self.cue;
                    self.preview_hot = None;
                    self.begin_audible();
                } else {
                    self.cue = self.quantize_pos(self.pos).round();
                    self.pos = self.cue;
                    sink.push(EngineEvent::CuePointSet {
                        deck: self.id,
                        position: Frames(self.cue as u64),
                    });
                }
            }
            (false, Transport::CuePreview) if self.preview_hot.is_none() => {
                self.stop_at(self.preview_return);
            }
            _ => {}
        }
    }

    pub(crate) fn set_cue_point(&mut self, position: Frames) {
        if self.track.is_some() {
            self.cue = (position.0 as f64).min(self.len_frames());
        }
    }

    /// Absolute move. Quantize does not apply; slip and scheduled jumps end.
    pub(crate) fn seek(&mut self, position: Frames) {
        if self.track.is_none() {
            return;
        }
        self.pending = None;
        self.slip_pos = None;
        self.slip_hold = 0;
        self.move_to(position.0 as f64);
    }

    pub(crate) fn set_tempo_fader(&mut self, position: f32) {
        if position.is_finite() {
            self.fader = position.clamp(-1.0, 1.0);
            self.held_rate = None;
        }
    }

    pub(crate) fn set_tempo_range(&mut self, range: TempoRange) {
        self.range = range;
        self.held_rate = None;
    }

    pub(crate) fn set_pitch_bend(&mut self, amount: f32) {
        if amount.is_finite() {
            self.bend = amount.clamp(-1.0, 1.0);
        }
    }

    pub(crate) fn set_key_lock(&mut self, on: bool) {
        if on == self.key_lock {
            return;
        }
        let was_audible = self.audible();
        if was_audible {
            self.start_tail();
        }
        self.key_lock = on;
        if was_audible {
            self.begin_audible();
        }
    }

    pub(crate) fn set_quantize(&mut self, on: bool) {
        self.quantize = on;
    }

    // ----- Transport helpers -----

    /// Moves the playhead to `target` with a click-free transition. No other
    /// state changes.
    fn move_to(&mut self, target: f64) {
        let target = if target.is_finite() { target } else { 0.0 };
        let target = target.clamp(0.0, self.len_frames());
        let was_audible = self.audible();
        if was_audible {
            self.start_tail();
        }
        self.pos = target;
        if was_audible {
            self.begin_audible();
        }
    }

    /// A jump that is part of the performance (cue, loop, hot cue, slip
    /// return): like [`Self::move_to`], and a beat-synced deck re-aligns.
    fn relocate(&mut self, target: f64) {
        self.move_to(target);
        self.mark_align();
    }

    /// Stops and parks the playhead at `position`, fading out what was playing.
    fn stop_at(&mut self, position: f64) {
        self.start_tail();
        self.transport = Transport::Stopped;
        self.preview_hot = None;
        self.pos = position.clamp(0.0, self.len_frames());
        self.on_stop();
    }

    /// Clears what only makes sense while playing.
    fn on_stop(&mut self) {
        self.pending = None;
        self.slip_pos = None;
        self.slip_hold = 0;
        if let Some(saved) = self.roll.take() {
            self.loop_stored = saved.stored;
            self.loop_active = saved.active;
        }
    }

    /// Called when output starts or restarts at `self.pos`: fade in, and
    /// re-prime the stretcher when key lock is on.
    fn begin_audible(&mut self) {
        self.rate_cur = self.target_rate();
        self.fade_in_left = self.declick;
        self.priming = None;
        if self.key_lock && self.scratch.is_none() {
            self.begin_priming();
        }
    }

    /// Renders `declick` frames of the current state with a fade-out into the
    /// tail buffer. The caller then changes state; the tail plays over it.
    fn start_tail(&mut self) {
        self.tail_len = 0;
        self.tail_pos = 0;
        self.tail_at = 0;
        if !self.audible() {
            return;
        }
        let n = self.declick;
        let rate = self.rate_cur;
        let mut buf = std::mem::take(&mut self.tail);
        if let Some(window) = buf.get_mut(..n * 2) {
            self.render_raw(window, n, RateRamp::constant(rate), None);
            for (i, frame) in window.chunks_exact_mut(2).enumerate() {
                let gain = 1.0 - (i + 1) as f32 / n as f32;
                frame[0] *= gain;
                frame[1] *= gain;
            }
            self.tail_len = n;
        }
        self.tail = buf;
    }

    /// Resets the stretcher and seeds it for a start at `self.pos`. The
    /// warm-up itself runs inside [`Self::render_data`], a bounded slice per
    /// block.
    ///
    /// Fed at rate `r`, a freshly reset stretcher has a fully formed output
    /// after `total = input_latency + output_latency` frames, and that output
    /// shows the input at `head0 + r * (total - output_latency) - input_latency`.
    /// Seeding at `head0 = aim + input_latency * (1 - r)` makes that the
    /// position `aim` the playhead reaches when the warm-up ends, `warm`
    /// frames from now. At rate 1 this is plain `head0 = aim`.
    fn begin_priming(&mut self) {
        let Some(track) = self.track.take() else {
            return;
        };
        let data = track.samples();
        let input_loop = self.input_loop();
        let rate = self.rate_cur.clamp(KEY_LOCK_MIN_RATE, MAX_RATE);
        let il = self.input_latency;
        let work = il + self.output_latency;
        let warm = work.div_ceil(PRIME_SPEEDUP);
        let aim = self.pos + rate * warm as f64;
        let head0 = wrap_head(aim + il as f64 * (1.0 - rate), input_loop);

        self.stretch.reset();
        if let Some(pre) = self.scratch_in.get_mut(..il * 2) {
            fill_input(data, head0.floor() as i64 - il as i64, pre, input_loop);
            self.stretch.seek(&*pre, rate);
        }
        self.head = head0;
        self.priming = Some(Priming {
            done: 0,
            warm,
            xfade: self.xfade,
            work_left: work,
            rate,
        });
        self.track = Some(track);
    }

    /// The loop the stretcher input follows, if a loop is active.
    fn input_loop(&self) -> Option<InputLoop> {
        if !self.loop_active || self.scratch.is_some() {
            return None;
        }
        let l = self.loop_stored?;
        let start = l.start.round() as i64;
        Some(InputLoop {
            start,
            end: l.end.round() as i64,
            xfade: self.declick.min(start.max(0) as usize),
        })
    }

    // ----- Rendering -----

    /// Runs the stretcher over `work` output frames at `rate`, discarding the
    /// output. Advances `self.head`.
    fn stretch_discard(&mut self, data: &[f32], work: usize, rate: f64) {
        let input_loop = self.input_loop();
        let mut remaining = work;
        while remaining > 0 {
            let n = remaining.min(PRIME_CHUNK);
            let next = self.head + n as f64 * rate;
            let i0 = self.head.floor();
            let count = (next.floor() - i0) as usize;
            if let (Some(input), Some(out)) = (
                self.scratch_in.get_mut(..count * 2),
                self.scratch_out.get_mut(..n * 2),
            ) {
                fill_input(data, i0 as i64, input, input_loop);
                self.stretch.process(&*input, out);
            }
            self.head = wrap_head(next, input_loop);
            remaining -= n;
        }
    }

    /// Renders `n` frames from the stretcher into `out` at `rate`, advancing
    /// `self.head`. Does not touch `self.pos`.
    fn stretch_live(&mut self, data: &[f32], out: &mut [f32], n: usize, rate: f64) {
        let input_loop = self.input_loop();
        let i0 = self.head.floor();
        let next = self.head + n as f64 * rate;
        let count = (next.floor() - i0) as usize;
        let Some(input) = self.scratch_in.get_mut(..count * 2) else {
            out.fill(0.0);
            return;
        };
        fill_input(data, i0 as i64, input, input_loop);
        self.stretch.process(&*input, &mut *out);
        self.head = wrap_head(next, input_loop);
    }

    /// Plain varispeed read of up to `n` frames, advancing `self.pos`.
    ///
    /// With `stop_at` the render ends on the first frame whose advance reaches
    /// that position (`hit`). With `scratch` the rate may be negative, the
    /// playhead is clamped to the track and the output fades with speed so a
    /// held record is silent instead of a DC offset.
    fn render_varispeed(
        &mut self,
        data: &[f32],
        out: &mut [f32],
        n: usize,
        ramp: RateRamp,
        opts: Varispeed,
    ) -> Rendered {
        let Varispeed {
            stop_at,
            scratch,
            looping,
        } = opts;
        let len = (data.len() / 2) as f64;
        let mut pos = self.pos;
        // A loop wraps only when the playhead reaches its end from below.
        let looping = looping.filter(|l| pos < l.end && l.len() > 0.0);
        let fade = looping.map_or(0.0, |l| (self.declick as f64).min(l.len() * 0.5));
        let mut ended = false;
        let mut hit = false;
        let mut frames = 0;
        for (i, frame) in out.chunks_exact_mut(2).take(n).enumerate() {
            let rate = ramp.start + ramp.step * (i + 1) as f64;
            if pos >= len || pos < 0.0 {
                frame.fill(0.0);
                ended |= pos >= len && !scratch;
            } else {
                let (mut l, mut r) = interpolate(data, pos, self.interpolation);
                if let Some(lp) = looping
                    && pos >= lp.end - fade
                {
                    // Approaching the loop end: blend into the audio before the
                    // loop start, which is what plays after the wrap.
                    let g = ((pos - (lp.end - fade)) / fade) as f32;
                    let (l2, r2) = interpolate(data, pos - lp.len(), self.interpolation);
                    l += (l2 - l) * g;
                    r += (r2 - r) * g;
                }
                let gain = if scratch {
                    (rate.abs() * SCRATCH_GAIN_SLOPE).min(1.0) as f32
                } else {
                    1.0
                };
                frame[0] = l * gain;
                frame[1] = r * gain;
            }
            pos += rate;
            frames = i + 1;
            if let Some(lp) = looping
                && pos >= lp.end
            {
                pos -= lp.len();
            }
            if scratch {
                pos = pos.clamp(0.0, len);
            } else if let Some(at) = stop_at
                && pos >= at
            {
                hit = true;
                break;
            }
        }
        if !scratch && pos >= len {
            ended = true;
            pos = len;
        }
        self.pos = pos;
        Rendered { frames, hit, ended }
    }

    /// Key lock start: varispeed while the stretcher warms up beside it, then
    /// a crossfade to the stretcher. Per block the stretcher does at most
    /// `PRIME_SPEEDUP * n` frames of warm-up plus the `n` it renders live.
    fn render_priming(
        &mut self,
        data: &[f32],
        mut p: Priming,
        out: &mut [f32],
        n: usize,
        ramp: RateRamp,
    ) -> Rendered {
        let rate = ramp.mean(n);
        let warm_frames = n.min(p.warm.saturating_sub(p.done));
        let work = p.work_left.min(warm_frames * PRIME_SPEEDUP);
        self.stretch_discard(data, work, p.rate);
        p.work_left -= work;

        let mut v = std::mem::take(&mut self.scratch_v);
        let result = if let Some(v) = v.get_mut(..n * 2) {
            let looping = self.active_loop();
            let opts = Varispeed {
                stop_at: None,
                scratch: false,
                looping,
            };
            let result = self.render_varispeed(data, v, n, ramp, opts);
            out.copy_from_slice(v);
            if let Some(live) = out.get_mut(warm_frames * 2..).filter(|l| !l.is_empty()) {
                self.stretch_live(data, live, n - warm_frames, rate);
                let frames = out.chunks_exact_mut(2).zip(v.chunks_exact(2));
                for (i, (o, v)) in frames.enumerate().skip(warm_frames) {
                    let since = p.done + i - p.warm;
                    let g = ((since + 1) as f32 / p.xfade as f32).min(1.0);
                    o[0] = o[0] * g + v[0] * (1.0 - g);
                    o[1] = o[1] * g + v[1] * (1.0 - g);
                }
            }
            result
        } else {
            out.fill(0.0);
            Rendered {
                frames: n,
                hit: false,
                ended: false,
            }
        };
        self.scratch_v = v;

        p.done += n;
        self.priming = (p.done < p.warm + p.xfade).then_some(p);
        result
    }

    /// Renders up to `n` frames into `out` (overwritten), advancing the
    /// playhead.
    fn render_raw(
        &mut self,
        out: &mut [f32],
        n: usize,
        ramp: RateRamp,
        plan: Option<Plan>,
    ) -> Rendered {
        let Some(track) = self.track.take() else {
            out.fill(0.0);
            return Rendered {
                frames: n,
                hit: false,
                ended: false,
            };
        };
        let rendered = self.render_data(track.samples(), out, n, ramp, plan);
        self.track = Some(track);
        rendered
    }

    fn render_data(
        &mut self,
        data: &[f32],
        out: &mut [f32],
        n: usize,
        ramp: RateRamp,
        plan: Option<Plan>,
    ) -> Rendered {
        let scratching = self.scratch.is_some();
        if !self.key_lock || scratching {
            let stop = plan.map(|p| p.at);
            let opts = Varispeed {
                stop_at: stop,
                scratch: scratching,
                looping: self.active_loop(),
            };
            return self.render_varispeed(data, out, n, ramp, opts);
        }
        let rate = ramp.mean(n);
        let whole = Rendered {
            frames: n,
            hit: false,
            ended: false,
        };
        if rate < KEY_LOCK_MIN_RATE {
            out.fill(0.0);
            return whole;
        }
        if let Some(p) = self.priming {
            return self.render_priming(data, p, out, n, ramp);
        }
        // A scheduled jump ends the render on its frame; a loop wraps silently.
        let (frames, hit) = match plan {
            Some(p) => {
                let k = ((p.at - self.pos) / rate).ceil().clamp(1.0, n as f64) as usize;
                (k, k < n || self.pos + k as f64 * rate >= p.at)
            }
            None => (n, false),
        };
        let armed = self.active_loop().filter(|l| self.pos < l.end);
        if let Some(window) = out.get_mut(..frames * 2) {
            self.stretch_live(data, window, frames, rate);
        }
        self.pos += frames as f64 * rate;
        if let Some(l) = armed
            && l.len() > 0.0
        {
            while self.pos >= l.end {
                self.pos -= l.len();
            }
        }
        let len = (data.len() / 2) as f64;
        let ended = self.pos >= len;
        if ended {
            self.pos = len;
        }
        Rendered { frames, hit, ended }
    }

    /// The active loop, unless the platter is being scratched.
    fn active_loop(&self) -> Option<Loop> {
        match (self.loop_active, self.loop_stored) {
            (true, Some(l)) if self.scratch.is_none() => Some(l),
            _ => None,
        }
    }

    /// The scheduled jump, if one is still ahead of the playhead.
    fn next_plan(&self) -> Option<Plan> {
        if self.scratch.is_some() {
            return None;
        }
        match self.pending {
            Some(p) if self.pos < p.at => Some(Plan { at: p.at, to: p.to }),
            _ => None,
        }
    }

    /// Carries out a jump the render stopped for. `frame` is the block frame
    /// the new stream starts at.
    fn take_hit(&mut self, plan: Plan, frame: usize) {
        let overshoot = (self.pos - plan.at).max(0.0);
        self.pending = None;
        self.start_tail();
        self.tail_at = frame;
        self.pos = (plan.to + overshoot).clamp(0.0, self.len_frames());
        self.begin_audible();
        self.mark_align();
    }

    /// Renders one block of interleaved stereo into `out` (overwritten).
    pub(crate) fn render(&mut self, out: &mut [f32], sink: &mut EventSink) {
        let n = out.len() / 2;
        self.prepare_jog(n);
        let natural = self.tempo_rate();
        let target = self.target_rate();
        let r0 = self.rate_cur;
        let step = (target - r0) / n.max(1) as f64;

        if self.audible() {
            let mut done = 0;
            let mut hits = 0;
            let mut ended = false;
            while done < n {
                let m = n - done;
                let plan = if hits < MAX_HITS_PER_BLOCK {
                    self.next_plan()
                } else {
                    None
                };
                if let Some(p) = self.pending
                    && self.scratch.is_none()
                    && self.pos >= p.at
                {
                    // Reached while priming or set in the past: go now.
                    let plan = Plan { at: p.at, to: p.to };
                    self.take_hit(plan, done);
                    hits += 1;
                    continue;
                }
                let ramp = RateRamp {
                    start: r0 + step * done as f64,
                    step,
                };
                let Some(segment) = out.get_mut(done * 2..) else {
                    break;
                };
                let result = self.render_raw(segment, m, ramp, plan);
                let rendered = result.frames.min(m);
                if let Some(head) = segment.get_mut(..rendered * 2) {
                    self.apply_fade_in(head);
                }
                done += rendered;
                if result.ended {
                    ended = true;
                    break;
                }
                if result.hit
                    && let Some(plan) = plan
                {
                    hits += 1;
                    self.take_hit(plan, done);
                } else if rendered == 0 {
                    break;
                }
            }
            if let Some(rest) = out.get_mut(done * 2..) {
                rest.fill(0.0);
            }
            self.rate_cur = target;
            if ended && self.scratch.is_none() {
                self.transport = Transport::Stopped;
                self.on_stop();
                sink.push(EngineEvent::TrackEnded { deck: self.id });
            }
        } else {
            out.fill(0.0);
            self.rate_cur = target;
        }

        if self.tail_pos < self.tail_len {
            let start = self.tail_at.min(n);
            let m = (n - start).min(self.tail_len - self.tail_pos);
            let src = self.tail.get(self.tail_pos * 2..(self.tail_pos + m) * 2);
            let dst = out.get_mut(start * 2..(start + m) * 2);
            if let (Some(src), Some(dst)) = (src, dst) {
                for (o, t) in dst.iter_mut().zip(src) {
                    *o += *t;
                }
            }
            self.tail_pos += m;
        }
        self.tail_at = 0;

        if let Some(shadow) = self.slip_pos.as_mut() {
            *shadow = (*shadow + natural * n as f64)
                .min(self.track.as_ref().map_or(0.0, |t| t.frames() as f64));
        }
    }

    fn apply_fade_in(&mut self, frames: &mut [f32]) {
        if self.fade_in_left == 0 {
            return;
        }
        let done = self.declick - self.fade_in_left;
        let m = (frames.len() / 2).min(self.fade_in_left);
        for (i, frame) in frames.chunks_exact_mut(2).take(m).enumerate() {
            let gain = (done + i + 1) as f32 / self.declick as f32;
            frame[0] *= gain;
            frame[1] *= gain;
        }
        self.fade_in_left -= m;
    }

    /// Fast-moving state for the next telemetry snapshot. The channel meter is
    /// filled in by the mixer.
    pub(crate) fn snapshot(&self) -> DeckTelemetry {
        let rate = self.rate_cur;
        let (bpm, beat_phase, beat_in_bar) = match self.grid {
            Some(grid) if self.track.is_some() => {
                let t = Seconds(self.pos / self.sample_rate);
                (
                    (rate > 0.0).then(|| Bpm(grid.bpm().0 * rate)),
                    grid.beat_phase(t) as f32,
                    grid.beat_in_bar(t),
                )
            }
            _ => (None, 0.0, 0),
        };
        DeckTelemetry {
            loaded: self.track.is_some(),
            playing: self.transport != Transport::Stopped,
            position: Frames(self.pos as u64),
            duration: self.track.as_ref().map_or(Frames::ZERO, |t| t.duration()),
            slip_position: self.slip_pos.map(|p| Frames(p as u64)),
            rate,
            bpm,
            beat_phase,
            beat_in_bar,
            active_loop: self.active_loop_frames(),
            ..DeckTelemetry::default()
        }
    }

    fn active_loop_frames(&self) -> Option<FrameRegion> {
        match (self.loop_active, self.loop_stored) {
            (true, Some(l)) => Some(FrameRegion {
                start: Frames(l.start as u64),
                end: Some(Frames(l.end as u64)),
            }),
            _ => None,
        }
    }

    /// Current state without touching the meters.
    pub(crate) fn status(&self) -> DeckStatus {
        DeckStatus {
            loaded: self.track.is_some(),
            playing: self.transport != Transport::Stopped,
            position: self.pos,
            rate: self.rate_cur,
            active_loop: match (self.loop_active, self.loop_stored) {
                (true, Some(l)) => Some((l.start, l.end)),
                _ => None,
            },
            slip_position: self.slip_pos,
            scratching: self.scratch.is_some(),
            sync: self.sync_status,
        }
    }
}

/// Output gain per unit of scratch speed: full level from a speed of
/// `1 / SCRATCH_GAIN_SLOPE` (a few percent of normal), so a held record is
/// silent rather than a DC offset.
const SCRATCH_GAIN_SLOPE: f64 = 40.0;

/// Wraps a stretcher input position that reached the loop end.
fn wrap_head(head: f64, input_loop: Option<InputLoop>) -> f64 {
    match input_loop {
        Some(l) if head >= l.end as f64 && l.end > l.start => {
            let len = (l.end - l.start) as f64;
            head - len * ((head - l.start as f64) / len).floor()
        }
        _ => head,
    }
}

/// Copies `dst.len() / 2` stereo frames starting at source frame `start`,
/// zero-filling outside the track. With a loop, frames at or past the loop end
/// read from the loop start instead, with a short crossfade into the seam.
fn fill_input(data: &[f32], start: i64, dst: &mut [f32], input_loop: Option<InputLoop>) {
    let frames = (data.len() / 2) as i64;
    let want = (dst.len() / 2) as i64;
    let Some(l) = input_loop.filter(|l| l.end > l.start && start + want > l.start) else {
        let from = start.clamp(0, frames);
        let to = (start + want).clamp(0, frames);
        let lead = (from - start).clamp(0, want) as usize;
        let copy = (to - from).max(0) as usize;
        dst.fill(0.0);
        if let (Some(src), Some(out)) = (
            data.get(from as usize * 2..(from as usize + copy) * 2),
            dst.get_mut(lead * 2..(lead + copy) * 2),
        ) {
            out.copy_from_slice(src);
        }
        return;
    };
    let len = l.end - l.start;
    for (k, frame) in dst.chunks_exact_mut(2).enumerate() {
        let mut i = start + k as i64;
        if i >= l.end {
            i = l.start + (i - l.start) % len;
        }
        let (mut a, mut b) = frame_at(data, i);
        let to_end = l.end - i;
        if i >= l.start && to_end <= l.xfade as i64 && l.xfade > 0 {
            // Fade from the loop tail into the audio just before the loop
            // start, which is what follows the loop end after a wrap.
            let g = 1.0 - to_end as f32 / (l.xfade as f32 + 1.0);
            let (c, d) = frame_at(data, i - len);
            a += (c - a) * g;
            b += (d - b) * g;
        }
        frame[0] = a;
        frame[1] = b;
    }
}

#[inline]
fn frame_at(data: &[f32], index: i64) -> (f32, f32) {
    if index < 0 {
        return (0.0, 0.0);
    }
    let i = index as usize * 2;
    match (data.get(i), data.get(i + 1)) {
        (Some(l), Some(r)) => (*l, *r),
        _ => (0.0, 0.0),
    }
}

/// Reads a stereo frame at fractional position `pos`.
#[inline]
fn interpolate(data: &[f32], pos: f64, mode: Interpolation) -> (f32, f32) {
    let base = pos.floor();
    let t = (pos - base) as f32;
    let i = base as i64;
    let (l1, r1) = frame_at(data, i);
    if t == 0.0 {
        return (l1, r1);
    }
    let (l2, r2) = frame_at(data, i + 1);
    match mode {
        Interpolation::Linear => (l1 + (l2 - l1) * t, r1 + (r2 - r1) * t),
        Interpolation::Cubic => {
            let (l0, r0) = frame_at(data, i - 1);
            let (l3, r3) = frame_at(data, i + 2);
            (
                catmull_rom(l0, l1, l2, l3, t),
                catmull_rom(r0, r1, r2, r3, t),
            )
        }
    }
}

#[inline]
fn catmull_rom(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3;
    let c = p2 - p0;
    p1 + 0.5 * t * (c + t * (b + t * a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolation_is_exact_on_integers_and_linear_between() {
        let data = [0.0, 0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0];
        for mode in [Interpolation::Linear, Interpolation::Cubic] {
            assert_eq!(interpolate(&data, 2.0, mode), (2.0, -2.0));
        }
        assert_eq!(interpolate(&data, 1.5, Interpolation::Linear), (1.5, -1.5));
        // Cubic reproduces a straight line.
        let (l, r) = interpolate(&data, 1.25, Interpolation::Cubic);
        assert!((l - 1.25).abs() < 1e-6 && (r + 1.25).abs() < 1e-6);
    }

    #[test]
    fn reads_outside_the_track_are_silent() {
        let data = [1.0, 1.0, 1.0, 1.0];
        assert_eq!(interpolate(&data, 5.0, Interpolation::Cubic), (0.0, 0.0));
        let mut dst = [9.0; 8];
        fill_input(&data, -1, &mut dst, None);
        assert_eq!(dst, [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn looped_input_wraps_to_the_loop_start() {
        let data: Vec<f32> = (0..20).flat_map(|i| [i as f32, i as f32]).collect();
        let l = InputLoop {
            start: 4,
            end: 8,
            xfade: 0,
        };
        let mut dst = [0.0; 12];
        fill_input(&data, 6, &mut dst, Some(l));
        let left: Vec<f32> = dst.chunks_exact(2).map(|f| f[0]).collect();
        assert_eq!(left, [6.0, 7.0, 4.0, 5.0, 6.0, 7.0]);
        assert_eq!(wrap_head(9.5, Some(l)), 5.5);
        assert_eq!(wrap_head(3.0, Some(l)), 3.0);
    }
}
