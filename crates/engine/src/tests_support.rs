//! Shared helpers for the offline render tests. Every render goes through
//! `EngineCore::render`, which runs under `assert_no_alloc` in test builds.

use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::HotCueIndex;
use opendeck_contracts::telemetry::EngineTelemetry;
use opendeck_contracts::{Bpm, DeckId, EngineCommand, Frames, SampleRate, Seconds};

use crate::{DeckStatus, EngineConfig, Interpolation, OfflineRenderer, TrackBuffer};

pub(crate) const SR: u32 = 48_000;
pub(crate) const BLOCK: usize = 512;

pub(crate) fn config(decks: u8) -> EngineConfig {
    EngineConfig {
        sample_rate: SampleRate(SR),
        max_block_frames: BLOCK as u32,
        deck_count: decks,
        interpolation: Interpolation::Cubic,
    }
}

pub(crate) fn deck(index: u8) -> DeckId {
    DeckId::new(index).expect("deck index in range")
}

pub(crate) fn pad(index: u8) -> HotCueIndex {
    HotCueIndex::new(index).expect("pad index in range")
}

pub(crate) fn grid(bpm: f64) -> BeatGrid {
    BeatGrid::new(Seconds(0.0), Bpm(bpm), 4).expect("valid grid")
}

/// Samples per beat at `bpm` (not an integer for most tempos).
pub(crate) fn beat_frames(bpm: f64) -> f64 {
    60.0 / bpm * f64::from(SR)
}

/// A click on every beat: a 2 ms burst of 1 kHz, decaying. Identical bursts,
/// so the track repeats exactly every beat when the beat is a whole number of
/// frames (120 BPM: 24000).
pub(crate) fn click_track(bpm: f64, seconds: f64) -> (TrackBuffer, BeatGrid) {
    let frames = (seconds * f64::from(SR)) as usize;
    let mut samples = vec![0.0_f32; frames * 2];
    let mut beat = 0u64;
    loop {
        let start = (beat as f64 * beat_frames(bpm)).round() as usize;
        if start >= frames {
            break;
        }
        for k in 0..96usize {
            let t = k as f64 / f64::from(SR);
            let v =
                0.8 * (-(k as f64) / 16.0).exp() * (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
            if let Some(frame) = samples.get_mut((start + k) * 2..(start + k) * 2 + 2) {
                frame[0] += v as f32;
                frame[1] += v as f32;
            }
        }
        beat += 1;
    }
    (
        TrackBuffer::from_vec(samples).expect("valid track"),
        grid(bpm),
    )
}

pub(crate) fn sine_samples(freq: f64, seconds: f64, amp: f32) -> Vec<f32> {
    let frames = (seconds * f64::from(SR)) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let v = amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(SR)).sin() as f32;
        out.push(v);
        out.push(v);
    }
    out
}

pub(crate) fn sine(freq: f64, seconds: f64) -> TrackBuffer {
    TrackBuffer::from_vec(sine_samples(freq, seconds, 0.5)).expect("valid track")
}

pub(crate) fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    (sum / samples.len().max(1) as f64).sqrt()
}

pub(crate) fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

pub(crate) fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

/// Left channel of interleaved stereo.
pub(crate) fn left(stereo: &[f32]) -> Vec<f32> {
    stereo.chunks_exact(2).map(|f| f[0]).collect()
}

/// Largest absolute difference between neighbouring left samples.
pub(crate) fn max_step(stereo: &[f32]) -> f32 {
    let l = left(stereo);
    l.windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

pub(crate) struct Rig {
    pub(crate) r: OfflineRenderer,
}

impl Rig {
    pub(crate) fn new(decks: u8) -> Self {
        Self {
            r: OfflineRenderer::new(config(decks)).expect("valid config"),
        }
    }

    pub(crate) fn send(&mut self, command: EngineCommand) {
        self.r
            .handle()
            .send(command)
            .expect("command ring has room");
    }

    pub(crate) fn load(&mut self, index: u8, track: TrackBuffer, grid: Option<BeatGrid>) {
        self.r
            .handle()
            .load_track_with_grid(deck(index), track, None, grid)
            .expect("hand-off ring has room");
        self.step(BLOCK);
    }

    pub(crate) fn load_click(&mut self, index: u8, bpm: f64, seconds: f64) {
        let (track, grid) = click_track(bpm, seconds);
        self.load(index, track, Some(grid));
    }

    /// Renders `frames` of master output.
    pub(crate) fn step(&mut self, frames: usize) -> Vec<f32> {
        self.r.render(frames)
    }

    pub(crate) fn status(&self, index: u8) -> DeckStatus {
        self.r.core().deck_status(deck(index)).expect("deck exists")
    }

    pub(crate) fn pos(&self, index: u8) -> f64 {
        self.status(index).position
    }

    /// A fresh telemetry snapshot. The telemetry ring holds the oldest
    /// snapshots when nobody polls, so drain it and render a little more.
    pub(crate) fn telemetry(&mut self) -> EngineTelemetry {
        while self.r.handle().poll_telemetry().is_some() {}
        for _ in 0..8 {
            self.step(BLOCK);
            if let Some(t) = self.r.handle().poll_telemetry() {
                return t;
            }
        }
        panic!("no telemetry snapshot")
    }

    pub(crate) fn play(&mut self, index: u8, playing: bool) {
        self.send(EngineCommand::SetPlaying {
            deck: deck(index),
            playing,
        });
    }

    pub(crate) fn seek(&mut self, index: u8, frame: u64) {
        self.send(EngineCommand::Seek {
            deck: deck(index),
            position: Frames(frame),
        });
    }

    pub(crate) fn quantize(&mut self, index: u8, on: bool) {
        self.send(EngineCommand::SetQuantize {
            deck: deck(index),
            on,
        });
    }
}
