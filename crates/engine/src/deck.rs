//! One deck: transport, CDJ-style CUE, tempo, pitch bend, key lock.
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
//! # Click-free jumps
//!
//! Stopping, seeking, cue jumps and key lock changes would click if the
//! output cut abruptly. Before such a change the deck renders a short tail of
//! the old state with a fade-out and mixes it over the first frames of the
//! new state, which fades in. Both last `declick` frames (about 2 ms).

use opendeck_contracts::telemetry::{DeckTelemetry, StereoMeter};
use opendeck_contracts::tempo::TempoRange;
use opendeck_contracts::{DeckId, EngineEvent, Frames, SampleRate};
use rtrb::Producer;
use signalsmith_stretch::Stretch;

use crate::meter::MeterAcc;
use crate::track::TrackBuffer;

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
    /// CUE held on the cue point: plays until release.
    CuePreview,
}

/// A deck's transport state as of the last rendered block.
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

/// One playback deck.
pub(crate) struct Deck {
    id: DeckId,
    track: Option<TrackBuffer>,
    transport: Transport,
    /// Audible playhead, fractional source frames.
    pos: f64,
    /// Main cue point, source frames.
    cue: f64,

    fader: f32,
    range: TempoRange,
    key_lock: bool,
    bend: f32,
    /// Rate used for the last rendered frame; ramps to the target per block.
    rate_cur: f64,
    interpolation: Interpolation,

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

    meter: MeterAcc,
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
            track: None,
            transport: Transport::Stopped,
            pos: 0.0,
            cue: 0.0,
            fader: 0.0,
            range: TempoRange::default(),
            key_lock: false,
            bend: 0.0,
            rate_cur: 1.0,
            interpolation,
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
            meter: MeterAcc::default(),
        }
    }

    fn len_frames(&self) -> f64 {
        self.track.as_ref().map_or(0.0, |t| t.frames() as f64)
    }

    fn audible(&self) -> bool {
        self.track.is_some() && self.transport != Transport::Stopped
    }

    fn target_rate(&self) -> f64 {
        (self.range.ratio(self.fader) * (1.0 + f64::from(self.bend) * MAX_PITCH_BEND))
            .clamp(0.0, MAX_RATE)
    }

    // ----- Commands -----

    /// Replaces the track. Returns the previous buffer for deferred free.
    pub(crate) fn load(
        &mut self,
        track: TrackBuffer,
        cue_point: Option<Frames>,
    ) -> Option<TrackBuffer> {
        let frames = track.frames() as f64;
        let old = self.track.replace(track);
        self.transport = Transport::Stopped;
        self.pos = 0.0;
        self.cue = cue_point.map_or(0.0, |f| (f.0 as f64).min(frames));
        self.tail_len = 0;
        self.tail_pos = 0;
        self.fade_in_left = 0;
        self.priming = None;
        self.rate_cur = self.target_rate();
        old
    }

    /// Unloads the track and returns it for deferred free.
    pub(crate) fn eject(&mut self) -> Option<TrackBuffer> {
        self.transport = Transport::Stopped;
        self.pos = 0.0;
        self.cue = 0.0;
        self.tail_len = 0;
        self.tail_pos = 0;
        self.fade_in_left = 0;
        self.priming = None;
        self.track.take()
    }

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
            }
            // PLAY while CUE is held latches playback.
            (true, Transport::CuePreview) => self.transport = Transport::Playing,
            (false, Transport::Playing | Transport::CuePreview) => {
                self.start_tail();
                self.transport = Transport::Stopped;
            }
            _ => {}
        }
    }

    /// CUE with CDJ behaviour.
    ///
    /// - Playing, press: return to the cue point and pause.
    /// - Paused off the cue point, press: set the cue point at the playhead.
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
                    self.begin_audible();
                } else {
                    self.cue = self.pos.round().clamp(0.0, self.len_frames());
                    sink.push(EngineEvent::CuePointSet {
                        deck: self.id,
                        position: Frames(self.cue as u64),
                    });
                }
            }
            (false, Transport::CuePreview) => self.stop_at(self.cue),
            _ => {}
        }
    }

    pub(crate) fn set_cue_point(&mut self, position: Frames) {
        if self.track.is_some() {
            self.cue = (position.0 as f64).min(self.len_frames());
        }
    }

    pub(crate) fn seek(&mut self, position: Frames) {
        if self.track.is_none() {
            return;
        }
        let target = (position.0 as f64).min(self.len_frames());
        let was_audible = self.audible();
        if was_audible {
            self.start_tail();
        }
        self.pos = target;
        if was_audible {
            self.begin_audible();
        }
    }

    pub(crate) fn set_tempo_fader(&mut self, position: f32) {
        if position.is_finite() {
            self.fader = position.clamp(-1.0, 1.0);
        }
    }

    pub(crate) fn set_tempo_range(&mut self, range: TempoRange) {
        self.range = range;
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

    // ----- Transport helpers -----

    /// Stops and parks the playhead at `position`, fading out what was playing.
    fn stop_at(&mut self, position: f64) {
        self.start_tail();
        self.transport = Transport::Stopped;
        self.pos = position.clamp(0.0, self.len_frames());
    }

    /// Called when output starts or restarts at `self.pos`: fade in, and
    /// re-prime the stretcher when key lock is on.
    fn begin_audible(&mut self) {
        self.rate_cur = self.target_rate();
        self.fade_in_left = self.declick;
        self.priming = None;
        if self.key_lock {
            self.begin_priming();
        }
    }

    /// Renders `declick` frames of the current state with a fade-out into the
    /// tail buffer. The caller then changes state; the tail plays over it.
    fn start_tail(&mut self) {
        self.tail_len = 0;
        self.tail_pos = 0;
        if !self.audible() {
            return;
        }
        let n = self.declick;
        let rate = self.rate_cur;
        let mut buf = std::mem::take(&mut self.tail);
        if let Some(window) = buf.get_mut(..n * 2) {
            self.render_raw(window, n, rate, rate);
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
    /// warm-up itself runs inside [`Self::render_raw`], a bounded slice per
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
        let rate = self.rate_cur.clamp(KEY_LOCK_MIN_RATE, MAX_RATE);
        let il = self.input_latency;
        let work = il + self.output_latency;
        let warm = work.div_ceil(PRIME_SPEEDUP);
        let aim = self.pos + rate * warm as f64;
        let head0 = aim + il as f64 * (1.0 - rate);

        self.stretch.reset();
        if let Some(pre) = self.scratch_in.get_mut(..il * 2) {
            fill_input(data, head0.floor() as i64 - il as i64, pre);
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

    // ----- Rendering -----

    /// Runs the stretcher over `work` output frames at `rate`, discarding the
    /// output. Advances `self.head`.
    fn stretch_discard(&mut self, data: &[f32], work: usize, rate: f64) {
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
                fill_input(data, i0 as i64, input);
                self.stretch.process(&*input, out);
            }
            self.head = next;
            remaining -= n;
        }
    }

    /// Renders `n` frames from the stretcher into `out` at `rate`, advancing
    /// `self.head`. Does not touch `self.pos`.
    fn stretch_live(&mut self, data: &[f32], out: &mut [f32], n: usize, rate: f64) {
        let i0 = self.head.floor();
        let next = self.head + n as f64 * rate;
        let count = (next.floor() - i0) as usize;
        let Some(input) = self.scratch_in.get_mut(..count * 2) else {
            out.fill(0.0);
            return;
        };
        fill_input(data, i0 as i64, input);
        self.stretch.process(&*input, &mut *out);
        self.head = next;
    }

    /// Plain varispeed read of `n` frames at a rate ramping `r0` to `r1`,
    /// advancing `self.pos`. Returns true when the end of the track was reached.
    fn render_varispeed(
        &mut self,
        data: &[f32],
        out: &mut [f32],
        n: usize,
        (r0, r1): (f64, f64),
    ) -> bool {
        let len = (data.len() / 2) as f64;
        let step = (r1 - r0) / n.max(1) as f64;
        let mut pos = self.pos;
        let mut ended = false;
        for (i, frame) in out.chunks_exact_mut(2).enumerate() {
            if pos >= len {
                frame.fill(0.0);
                ended = true;
                continue;
            }
            let (l, r) = interpolate(data, pos, self.interpolation);
            frame[0] = l;
            frame[1] = r;
            pos += r0 + step * (i + 1) as f64;
        }
        if pos >= len {
            ended = true;
            pos = len;
        }
        self.pos = pos;
        ended
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
        rates: (f64, f64),
    ) -> bool {
        let rate = 0.5 * (rates.0 + rates.1);
        let warm_frames = n.min(p.warm.saturating_sub(p.done));
        let work = p.work_left.min(warm_frames * PRIME_SPEEDUP);
        self.stretch_discard(data, work, p.rate);
        p.work_left -= work;

        let mut v = std::mem::take(&mut self.scratch_v);
        let ended = if let Some(v) = v.get_mut(..n * 2) {
            let ended = self.render_varispeed(data, v, n, rates);
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
            ended
        } else {
            out.fill(0.0);
            false
        };
        self.scratch_v = v;

        p.done += n;
        self.priming = (p.done < p.warm + p.xfade).then_some(p);
        ended
    }

    /// Renders `n` frames into `out` (`n * 2` samples, overwritten), advancing
    /// the playhead. Returns true when the end of the track was reached.
    fn render_raw(&mut self, out: &mut [f32], n: usize, r0: f64, r1: f64) -> bool {
        let Some(track) = self.track.take() else {
            out.fill(0.0);
            return false;
        };
        let ended = self.render_data(track.samples(), out, n, (r0, r1));
        self.track = Some(track);
        ended
    }

    fn render_data(&mut self, data: &[f32], out: &mut [f32], n: usize, rates: (f64, f64)) -> bool {
        if !self.key_lock {
            return self.render_varispeed(data, out, n, rates);
        }
        let rate = 0.5 * (rates.0 + rates.1);
        if rate < KEY_LOCK_MIN_RATE {
            out.fill(0.0);
            return false;
        }
        if let Some(p) = self.priming {
            return self.render_priming(data, p, out, n, rates);
        }
        self.stretch_live(data, out, n, rate);
        self.pos += n as f64 * rate;
        let len = (data.len() / 2) as f64;
        if self.pos >= len {
            self.pos = len;
            return true;
        }
        false
    }

    /// Renders one block of interleaved stereo into `out` (overwritten).
    pub(crate) fn render(&mut self, out: &mut [f32], sink: &mut EventSink) {
        let n = out.len() / 2;
        let target = self.target_rate();
        if self.audible() {
            let ended = self.render_raw(out, n, self.rate_cur, target);
            self.rate_cur = target;
            if self.fade_in_left > 0 {
                let done = self.declick - self.fade_in_left;
                let m = n.min(self.fade_in_left);
                for (i, frame) in out.chunks_exact_mut(2).take(m).enumerate() {
                    let gain = (done + i + 1) as f32 / self.declick as f32;
                    frame[0] *= gain;
                    frame[1] *= gain;
                }
                self.fade_in_left -= m;
            }
            if ended {
                self.transport = Transport::Stopped;
                sink.push(EngineEvent::TrackEnded { deck: self.id });
            }
        } else {
            out.fill(0.0);
            self.rate_cur = target;
        }

        if self.tail_pos < self.tail_len {
            let m = n.min(self.tail_len - self.tail_pos);
            let src = self.tail.get(self.tail_pos * 2..(self.tail_pos + m) * 2);
            if let Some(src) = src {
                for (o, t) in out.iter_mut().zip(src) {
                    *o += *t;
                }
            }
            self.tail_pos += m;
        }

        self.meter.add(out);
    }

    /// Fast-moving state for the next telemetry snapshot. Takes the meter.
    pub(crate) fn snapshot(&mut self) -> DeckTelemetry {
        let meter: StereoMeter = self.meter.take();
        DeckTelemetry {
            loaded: self.track.is_some(),
            playing: self.transport != Transport::Stopped,
            position: Frames(self.pos as u64),
            duration: self.track.as_ref().map_or(Frames::ZERO, |t| t.duration()),
            rate: self.rate_cur,
            meter,
            ..DeckTelemetry::default()
        }
    }

    /// Current state without touching the meters.
    pub(crate) fn status(&self) -> DeckStatus {
        DeckStatus {
            loaded: self.track.is_some(),
            playing: self.transport != Transport::Stopped,
            position: self.pos,
            rate: self.rate_cur,
        }
    }
}

/// Copies `dst.len() / 2` stereo frames starting at source frame `start`,
/// zero-filling outside the track.
fn fill_input(data: &[f32], start: i64, dst: &mut [f32]) {
    let frames = (data.len() / 2) as i64;
    let want = (dst.len() / 2) as i64;
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
        fill_input(&data, -1, &mut dst);
        assert_eq!(dst, [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
    }
}
