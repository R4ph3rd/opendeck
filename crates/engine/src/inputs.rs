//! Microphone and aux inputs: the input ring, the mic channel (level, 2-band
//! EQ, on/off), the aux channel (level, fader) and talk-over ducking.
//!
//! # Transport
//!
//! An input stream (a second cpal stream; cpal has no duplex stream, so even
//! one device is two callbacks) pushes interleaved frames of
//! [`INPUT_CHANNELS`] samples (mic L, mic R, aux L, aux R) into an SPSC ring
//! with [`InputProducer::push_frames`]. The engine pops one block of frames per
//! render. Neither side locks or allocates.
//!
//! The two callbacks run on their own schedules, so the engine keeps a small
//! cushion: it waits until `prefill` frames are queued before it starts
//! consuming, and when the queue runs dry it inserts silence and primes again
//! (counted as an underrun). When the producer runs ahead (the input clock is
//! faster, or the output stalled) the queue is cut back to `prefill` and the
//! dropped frames are counted as overruns. Audio from a separate input device
//! therefore drifts against the output clock by the clock error until a cut or
//! a silence insert; an asynchronous resampler would remove that and is
//! post-MVP (`CONTEXT/engine.md`).
//!
//! Latency of the mic path is the input buffer, plus the prefill (set from the
//! larger of the input and output buffer), plus the output buffer.
//!
//! # Signal flow
//!
//! ```text
//! mic  -> level -> 2-band EQ -> meter -> on/off -----------+--> + -> master level
//! aux  -> level -> fader -> meter ---------------+         |
//! program (decks, sampler, FX) ------------------+--> * duck
//! mic (post switch) -> level detector -> duck gain -> program
//! ```
//!
//! The inputs join the master after the master FX and before the master
//! level, so the master level, booth and headphone master tap all hear them.
//!
//! # Product choices (not external specs)
//!
//! - Mic EQ: low shelf at 120 Hz and high shelf at 5 kHz (RBJ cookbook,
//!   slope 1). Gain is linear, unity is flat, kill is about -26 dB.
//! - Talk-over: instant-attack peak detector with a 150 ms release; the
//!   program gain moves down with a 10 ms time constant and back up with a
//!   400 ms one. Mic must be on.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use opendeck_contracts::EngineCommand;
use opendeck_contracts::mixer::{EQ_MAX_GAIN_DB, EqBand, TRIM_MAX_GAIN_DB};
use opendeck_contracts::telemetry::InputTelemetry;
use opendeck_contracts::units::db_to_gain;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::dsp::{Biquad, Shelf};
use crate::meter::MeterAcc;
use crate::mixer::Ramp;

/// Samples per input frame: mic left, mic right, aux left, aux right.
pub const INPUT_CHANNELS: usize = 4;

/// Capacity of the input ring in frames (about 170 ms at 48 kHz).
pub const INPUT_RING_FRAMES: usize = 8192;

/// Low shelf corner of the mic EQ, Hz.
const MIC_LOW_HZ: f64 = 120.0;
/// High shelf corner of the mic EQ, Hz.
const MIC_HIGH_HZ: f64 = 5_000.0;
/// Mic EQ gain that counts as a kill, dB.
const MIC_KILL_DB: f32 = -26.0;
/// Frames between mic EQ coefficient updates while a gain moves.
const EQ_STEP: usize = 32;
/// Detector release, seconds.
const ENV_RELEASE_S: f32 = 0.150;
/// Time constant of the program gain going down, seconds.
const DUCK_ATTACK_S: f32 = 0.010;
/// Time constant of the program gain coming back, seconds.
const DUCK_RELEASE_S: f32 = 0.400;
/// Largest talk-over depth, dB.
const MAX_DEPTH_DB: f32 = 40.0;

/// Counters and tuning shared by the two ends of the input ring.
#[derive(Debug, Default)]
pub(crate) struct InputShared {
    overruns: AtomicU32,
    underruns: AtomicU32,
    prefill: AtomicU32,
}

impl InputShared {
    pub(crate) fn set_prefill(&self, frames: u32) {
        self.prefill.store(frames, Ordering::Relaxed);
    }
}

/// Writer end of the input ring. Owned by the input stream callback.
pub struct InputProducer {
    ring: Producer<f32>,
    shared: Arc<InputShared>,
}

impl InputProducer {
    /// Queues whole frames of [`INPUT_CHANNELS`] interleaved samples and
    /// returns how many frames were accepted. Frames that do not fit are
    /// dropped and counted as overruns. Real-time safe.
    pub fn push_frames(&mut self, interleaved: &[f32]) -> usize {
        let whole = interleaved.len() - interleaved.len() % INPUT_CHANNELS;
        let room = self.ring.slots() - self.ring.slots() % INPUT_CHANNELS;
        let take = whole.min(room);
        let pushed = interleaved
            .get(..take)
            .map_or(0, |data| self.ring.push_partial_slice(data).0.len());
        let dropped = (whole - pushed) / INPUT_CHANNELS;
        if dropped > 0 {
            self.shared.overruns.fetch_add(
                u32::try_from(dropped).unwrap_or(u32::MAX),
                Ordering::Relaxed,
            );
        }
        pushed / INPUT_CHANNELS
    }

    /// Frames the ring can take right now.
    pub fn free_frames(&self) -> usize {
        self.ring.slots() / INPUT_CHANNELS
    }
}

/// Builds the ring: the writer for the input stream, the reader for the core.
pub(crate) fn input_ring() -> (InputProducer, Consumer<f32>, Arc<InputShared>) {
    let (producer, consumer) = RingBuffer::new(INPUT_RING_FRAMES * INPUT_CHANNELS);
    let shared = Arc::new(InputShared::default());
    (
        InputProducer {
            ring: producer,
            shared: Arc::clone(&shared),
        },
        consumer,
        shared,
    )
}

/// Audio-thread input state.
pub(crate) struct InputRt {
    ring: Consumer<f32>,
    shared: Arc<InputShared>,
    sample_rate: f64,
    /// Frames popped from the ring this block.
    raw: Vec<f32>,
    /// Mic after level, EQ and switch (interleaved stereo).
    mic: Vec<f32>,
    /// Aux after level and fader (interleaved stereo).
    aux: Vec<f32>,
    primed: bool,
    seen: bool,
    active: bool,

    mic_level: Ramp,
    mic_on: Ramp,
    mic_on_target: bool,
    eq_target_db: [f32; 2],
    eq_db: [f32; 2],
    eq_applied_db: [f32; 2],
    /// `[channel][band]`, band 0 low shelf, 1 high shelf.
    shelves: [[Biquad; 2]; 2],
    aux_level: Ramp,
    aux_fader: Ramp,

    talk_on: bool,
    depth_gain: f32,
    threshold: f32,
    env: f32,
    duck_gain: f32,
    duck_min: f32,
    env_release: f32,
    duck_attack: f32,
    duck_release: f32,

    mic_meter: MeterAcc,
    aux_meter: MeterAcc,
}

impl InputRt {
    /// Allocates the scratch buffers. Not real-time safe.
    pub(crate) fn new(
        ring: Consumer<f32>,
        shared: Arc<InputShared>,
        sample_rate: f64,
        max_block_frames: usize,
    ) -> Self {
        let frames = max_block_frames.max(1);
        let coeff = |seconds: f32| (-1.0 / (seconds * sample_rate as f32).max(1.0)).exp();
        let flat = |kind, fc| Biquad::shelf(kind, fc, 0.0, sample_rate);
        Self {
            ring,
            shared,
            sample_rate,
            raw: vec![0.0; frames * INPUT_CHANNELS],
            mic: vec![0.0; frames * 2],
            aux: vec![0.0; frames * 2],
            primed: false,
            seen: false,
            active: false,
            mic_level: Ramp::new(1.0),
            mic_on: Ramp::new(1.0),
            mic_on_target: true,
            eq_target_db: [0.0; 2],
            eq_db: [0.0; 2],
            eq_applied_db: [0.0; 2],
            shelves: [[flat(Shelf::Low, MIC_LOW_HZ), flat(Shelf::High, MIC_HIGH_HZ)]; 2],
            aux_level: Ramp::new(1.0),
            aux_fader: Ramp::new(1.0),
            talk_on: false,
            depth_gain: db_to_gain(-12.0),
            threshold: db_to_gain(-40.0),
            env: 0.0,
            duck_gain: 1.0,
            duck_min: 1.0,
            env_release: coeff(ENV_RELEASE_S),
            duck_attack: coeff(DUCK_ATTACK_S),
            duck_release: coeff(DUCK_RELEASE_S),
            mic_meter: MeterAcc::default(),
            aux_meter: MeterAcc::default(),
        }
    }

    /// Applies an input command. Returns false for any other command.
    pub(crate) fn apply(&mut self, command: EngineCommand) -> bool {
        use EngineCommand as C;
        match command {
            C::SetMicLevel { gain } => {
                if gain.0.is_finite() {
                    self.mic_level.set(gain.0.min(db_to_gain(TRIM_MAX_GAIN_DB)));
                }
            }
            C::SetMicEq { band, gain } => {
                let slot = match band {
                    EqBand::Low => Some(0),
                    EqBand::High => Some(1),
                    EqBand::Mid => None,
                };
                if let Some(slot) = slot.and_then(|i| self.eq_target_db.get_mut(i))
                    && gain.0.is_finite()
                {
                    let linear = gain.0.clamp(0.0, db_to_gain(EQ_MAX_GAIN_DB));
                    *slot = if linear <= 0.0 {
                        MIC_KILL_DB
                    } else {
                        (20.0 * linear.log10()).max(MIC_KILL_DB)
                    };
                }
            }
            C::SetMicOn { on } => self.mic_on_target = on,
            C::SetAuxLevel { gain } => {
                if gain.0.is_finite() {
                    self.aux_level.set(gain.0.min(db_to_gain(TRIM_MAX_GAIN_DB)));
                }
            }
            C::SetAuxFader { position } => {
                if position.is_finite() {
                    self.aux_fader.set(position.clamp(0.0, 1.0));
                }
            }
            C::SetTalkOver {
                on,
                depth_db,
                threshold_db,
            } => {
                self.talk_on = on;
                if depth_db.is_finite() {
                    self.depth_gain = db_to_gain(-depth_db.clamp(0.0, MAX_DEPTH_DB));
                }
                if threshold_db.is_finite() {
                    self.threshold = db_to_gain(threshold_db.clamp(-80.0, 0.0));
                }
            }
            _ => return false,
        }
        true
    }

    /// Pops this block's input frames and prepares the mic and aux signals.
    /// Cheap and silent until the first frames arrive.
    pub(crate) fn begin_block(&mut self, frames: usize) {
        self.active = false;
        let frames = frames.min(self.mic.len() / 2);
        let available = self.ring.slots() / INPUT_CHANNELS;
        if available == 0 && self.ring.is_abandoned() {
            self.primed = false;
            // A closed input stream while ducking: finish the release on silence.
            if self.seen && self.duck_gain < 0.999 {
                self.raw.fill(0.0);
                self.active = true;
                self.process(frames);
            }
            return;
        }
        let prefill = (self.shared.prefill.load(Ordering::Relaxed) as usize).max(frames);
        if !self.primed && available >= prefill {
            self.primed = true;
        }
        let wanted = frames * INPUT_CHANNELS;
        let Some(block) = self.raw.get_mut(..wanted) else {
            return;
        };
        if self.primed {
            let (popped, rest) = self.ring.pop_partial_slice(block);
            let got = popped.len();
            rest.fill(0.0);
            self.seen = true;
            if got < wanted {
                self.primed = false;
                self.shared.underruns.fetch_add(1, Ordering::Relaxed);
            } else {
                // Producer ran ahead: cut back to the cushion.
                let left = self.ring.slots() / INPUT_CHANNELS;
                if left > prefill * 2 + frames {
                    let cut = left - prefill;
                    if let Ok(chunk) = self.ring.read_chunk(cut * INPUT_CHANNELS) {
                        chunk.commit_all();
                    }
                    self.shared
                        .overruns
                        .fetch_add(u32::try_from(cut).unwrap_or(u32::MAX), Ordering::Relaxed);
                }
            }
        } else {
            block.fill(0.0);
        }
        if self.seen {
            self.active = true;
            self.process(frames);
        }
    }

    fn process(&mut self, frames: usize) {
        let (Some(raw), Some(mic), Some(aux)) = (
            self.raw.get(..frames * INPUT_CHANNELS),
            self.mic.get_mut(..frames * 2),
            self.aux.get_mut(..frames * 2),
        ) else {
            return;
        };

        // Mic: level.
        let (g0, gstep) = self.mic_level.advance(frames);
        for (i, (out, input)) in mic
            .chunks_exact_mut(2)
            .zip(raw.chunks_exact(INPUT_CHANNELS))
            .enumerate()
        {
            let g = g0 + gstep * (i + 1) as f32;
            out[0] = input[0] * g;
            out[1] = input[1] * g;
        }

        // Mic: 2-band EQ, skipped while flat at rest.
        let from = self.eq_db;
        let to = self.eq_target_db;
        self.eq_db = to;
        let flat = from.iter().chain(&to).all(|d| *d == 0.0);
        if !flat {
            for (band, (&d0, &d1)) in from.iter().zip(&to).enumerate() {
                if d0 == d1 && self.eq_applied_db[band] != d1 {
                    retune(&mut self.shelves, self.sample_rate, band, d1);
                    self.eq_applied_db[band] = d1;
                }
            }
            let mut start = 0;
            while start < frames {
                let n = EQ_STEP.min(frames - start);
                let mid = (start + n / 2) as f32 / frames as f32;
                for (band, (&d0, &d1)) in from.iter().zip(&to).enumerate() {
                    if d0 != d1 {
                        retune(
                            &mut self.shelves,
                            self.sample_rate,
                            band,
                            d0 + (d1 - d0) * mid,
                        );
                        // The next block settles exactly on the target.
                        self.eq_applied_db[band] = f32::NAN;
                    }
                }
                if let Some(chunk) = mic.get_mut(start * 2..(start + n) * 2) {
                    for frame in chunk.chunks_exact_mut(2) {
                        for (sample, shelves) in frame.iter_mut().zip(self.shelves.iter_mut()) {
                            let x = shelves
                                .iter_mut()
                                .fold(f64::from(*sample), |acc, shelf| shelf.tick(acc));
                            *sample = x as f32;
                        }
                    }
                }
                start += n;
            }
        }
        self.mic_meter.add(mic);

        // Mic: on/off ramp.
        self.mic_on.set(if self.mic_on_target { 1.0 } else { 0.0 });
        let (o0, ostep) = self.mic_on.advance(frames);
        if !(o0 == 1.0 && ostep == 0.0) {
            for (i, frame) in mic.chunks_exact_mut(2).enumerate() {
                let g = o0 + ostep * (i + 1) as f32;
                frame[0] *= g;
                frame[1] *= g;
            }
        }

        // Aux: level and fader.
        let (l0, lstep) = self.aux_level.advance(frames);
        let (f0, fstep) = self.aux_fader.advance(frames);
        for (i, (out, input)) in aux
            .chunks_exact_mut(2)
            .zip(raw.chunks_exact(INPUT_CHANNELS))
            .enumerate()
        {
            let t = (i + 1) as f32;
            let g = (l0 + lstep * t) * (f0 + fstep * t);
            out[0] = input[2] * g;
            out[1] = input[3] * g;
        }
        self.aux_meter.add(aux);
    }

    /// Adds aux and mic to `master` (interleaved stereo, the program) and
    /// ducks the program while talk-over triggers. Call after the master FX.
    pub(crate) fn mix_into(&mut self, master: &mut [f32]) {
        if !self.active {
            return;
        }
        let frames = (master.len() / 2).min(self.mic.len() / 2);
        let (Some(mic), Some(aux), Some(master)) = (
            self.mic.get(..frames * 2),
            self.aux.get(..frames * 2),
            master.get_mut(..frames * 2),
        ) else {
            return;
        };
        for ((out, m), a) in master
            .chunks_exact_mut(2)
            .zip(mic.chunks_exact(2))
            .zip(aux.chunks_exact(2))
        {
            let level = m[0].abs().max(m[1].abs());
            self.env = level.max(self.env * self.env_release);
            let target = if self.talk_on && self.env > self.threshold {
                self.depth_gain
            } else {
                1.0
            };
            let coeff = if target < self.duck_gain {
                self.duck_attack
            } else {
                self.duck_release
            };
            self.duck_gain = target + (self.duck_gain - target) * coeff;
            if (self.duck_gain - target).abs() < 1e-6 {
                self.duck_gain = target;
            }
            self.duck_min = self.duck_min.min(self.duck_gain);
            out[0] = (out[0] + a[0]) * self.duck_gain + m[0];
            out[1] = (out[1] + a[1]) * self.duck_gain + m[1];
        }
    }

    /// Snapshot for telemetry; starts a new meter window.
    pub(crate) fn take_telemetry(&mut self) -> InputTelemetry {
        let gone = self.ring.is_abandoned() && self.ring.is_empty();
        let duck_db = if self.duck_min < 1.0 {
            -20.0 * self.duck_min.max(1e-4).log10()
        } else {
            0.0
        };
        self.duck_min = self.duck_gain;
        InputTelemetry {
            connected: self.seen && !gone,
            mic: self.mic_meter.take(),
            aux: self.aux_meter.take(),
            duck_db,
            underruns: self.shared.underruns.load(Ordering::Relaxed),
            overruns: self.shared.overruns.load(Ordering::Relaxed),
        }
    }
}

/// Retunes band `band` (0 low shelf, 1 high shelf) of both channels to `db`,
/// keeping the filter states.
fn retune(shelves: &mut [[Biquad; 2]; 2], sample_rate: f64, band: usize, db: f32) {
    let (kind, fc) = if band == 0 {
        (Shelf::Low, MIC_LOW_HZ)
    } else {
        (Shelf::High, MIC_HIGH_HZ)
    };
    let coeffs = Biquad::shelf(kind, fc, f64::from(db), sample_rate);
    for channel in shelves {
        if let Some(shelf) = channel.get_mut(band) {
            shelf.retune(&coeffs);
        }
    }
}
