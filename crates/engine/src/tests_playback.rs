//! Offline render tests for deck playback. Every render goes through
//! `EngineCore::render`, which runs under `assert_no_alloc` in test builds.

use opendeck_contracts::tempo::TempoRange;
use opendeck_contracts::{DeckId, EngineCommand, EngineEvent, Frames, Gain, SampleRate};

use crate::{
    EngineConfig, Interpolation, MAX_PITCH_BEND, OfflineRenderer, TrackBuffer, TrackError,
};

const SR: u32 = 48_000;
const BLOCK: usize = 512;

fn config() -> EngineConfig {
    EngineConfig {
        sample_rate: SampleRate(SR),
        max_block_frames: BLOCK as u32,
        deck_count: 2,
        interpolation: Interpolation::Cubic,
    }
}

fn deck(index: u8) -> DeckId {
    DeckId::new(index).expect("deck index in range")
}

/// Interleaved stereo sine, same signal on both channels.
fn sine_samples(freq: f64, seconds: f64, amp: f32) -> Vec<f32> {
    let frames = (seconds * f64::from(SR)) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let v = amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(SR)).sin() as f32;
        out.push(v);
        out.push(v);
    }
    out
}

fn sine(freq: f64, seconds: f64) -> TrackBuffer {
    TrackBuffer::from_vec(sine_samples(freq, seconds, 0.5)).expect("valid track")
}

/// Positive-going zero crossings with linear interpolation: frequency in Hz
/// of the left channel of `stereo`.
fn estimate_frequency(stereo: &[f32]) -> f64 {
    let mono: Vec<f32> = stereo.chunks_exact(2).map(|f| f[0]).collect();
    let mut first = None;
    let mut last = 0.0;
    let mut count = 0usize;
    for (i, w) in mono.windows(2).enumerate() {
        if w[0] < 0.0 && w[1] >= 0.0 {
            let t = i as f64 + f64::from(-w[0]) / f64::from(w[1] - w[0]);
            first.get_or_insert(t);
            last = t;
            count += 1;
        }
    }
    match first {
        Some(first) if count > 1 => (count - 1) as f64 * f64::from(SR) / (last - first),
        _ => 0.0,
    }
}

fn rms(stereo: &[f32]) -> f64 {
    let sum: f64 = stereo.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    (sum / stereo.len().max(1) as f64).sqrt()
}

fn peak(stereo: &[f32]) -> f32 {
    stereo.iter().fold(0.0, |m, s| m.max(s.abs()))
}

struct Rig {
    r: OfflineRenderer,
}

impl Rig {
    fn new() -> Self {
        Self::with(config())
    }

    fn with(config: EngineConfig) -> Self {
        Self {
            r: OfflineRenderer::new(config).expect("valid config"),
        }
    }

    fn send(&mut self, command: EngineCommand) {
        self.r
            .handle()
            .send(command)
            .expect("command ring has room");
    }

    fn load(&mut self, index: u8, track: TrackBuffer) {
        self.r
            .handle()
            .load_track(deck(index), track, None)
            .expect("hand-off ring has room");
        self.step(BLOCK);
    }

    fn step(&mut self, frames: usize) -> Vec<f32> {
        self.r.render(frames)
    }

    fn pos(&self, index: u8) -> f64 {
        self.r
            .core()
            .deck_status(deck(index))
            .expect("deck exists")
            .position
    }

    fn playing(&self, index: u8) -> bool {
        self.r
            .core()
            .deck_status(deck(index))
            .expect("deck exists")
            .playing
    }

    fn play(&mut self, index: u8, playing: bool) {
        self.send(EngineCommand::SetPlaying {
            deck: deck(index),
            playing,
        });
    }

    fn cue(&mut self, pressed: bool) {
        self.send(EngineCommand::Cue {
            deck: deck(0),
            pressed,
        });
    }

    fn seek(&mut self, frame: u64) {
        self.send(EngineCommand::Seek {
            deck: deck(0),
            position: Frames(frame),
        });
    }

    fn tempo(&mut self, range: TempoRange, fader: f32) {
        self.send(EngineCommand::SetTempoRange {
            deck: deck(0),
            range,
        });
        self.send(EngineCommand::SetTempoFader {
            deck: deck(0),
            position: fader,
        });
    }

    fn key_lock(&mut self, on: bool) {
        self.send(EngineCommand::SetKeyLock { deck: deck(0), on });
    }
}

// ----- Playback, tempo -----

#[test]
fn plays_a_sine_at_zero_percent() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    let out = rig.step(2 * SR as usize);

    assert_eq!(rig.pos(0), f64::from(2 * SR) + 0.0);
    let settled = &out[4096..];
    let freq = estimate_frequency(settled);
    assert!((freq - 1000.0).abs() < 5.0, "frequency {freq}");
    let level = rms(settled);
    assert!((level - 0.5 / 2f64.sqrt()).abs() < 0.005, "rms {level}");
    assert!((peak(settled) - 0.5).abs() < 0.005);
}

#[test]
fn varispeed_plus_eight_percent_moves_pitch_and_playhead() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.tempo(TempoRange::Percent10, 0.8);
    rig.play(0, true);
    let out = rig.step(2 * SR as usize);

    let ratio = TempoRange::Percent10.ratio(0.8);
    let expected = f64::from(2 * SR) * ratio;
    assert!(
        (rig.pos(0) - expected).abs() < 1e-3,
        "playhead {} expected {expected}",
        rig.pos(0)
    );
    let freq = estimate_frequency(&out[4096..]);
    assert!(
        (freq - 1000.0 * ratio).abs() < 1000.0 * ratio * 0.005,
        "{freq}"
    );
    assert!((rms(&out[4096..]) - 0.5 / 2f64.sqrt()).abs() < 0.005);
}

#[test]
fn output_is_bit_exact_at_unity_rate() {
    let samples = sine_samples(440.0, 1.0, 0.5);
    let mut rig = Rig::new();
    rig.load(0, TrackBuffer::from_vec(samples.clone()).expect("track"));
    rig.play(0, true);
    let out = rig.step(SR as usize / 2);
    // Past the 2 ms fade-in the engine copies the track untouched.
    assert_eq!(&out[1000..20_000], &samples[1000..20_000]);
}

#[test]
fn tempo_ranges_scale_the_fader() {
    for (range, fader, expected) in [
        (TempoRange::Percent6, 1.0, 1.06),
        (TempoRange::Percent6, -1.0, 0.94),
        (TempoRange::Percent10, 1.0, 1.10),
        (TempoRange::Percent16, -1.0, 0.84),
        (TempoRange::Wide, 1.0, 2.0),
        (TempoRange::Wide, -1.0, 0.0),
    ] {
        let mut rig = Rig::new();
        rig.load(0, sine(1000.0, 5.0));
        rig.tempo(range, fader);
        rig.play(0, true);
        rig.step(SR as usize);
        assert!(
            (rig.pos(0) - f64::from(SR) * expected).abs() < 1e-3,
            "{range:?} {fader}: {}",
            rig.pos(0)
        );
    }
}

#[test]
fn pitch_bend_is_a_temporary_rate_offset() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    rig.step(BLOCK);
    let before = rig.pos(0);
    rig.send(EngineCommand::PitchBend {
        deck: deck(0),
        amount: 1.0,
    });
    // The rate ramps over the first block; measure once settled.
    rig.step(BLOCK);
    let settled = rig.pos(0);
    rig.step(SR as usize / 2);
    let advance = rig.pos(0) - settled;
    let expected = f64::from(SR / 2) * (1.0 + MAX_PITCH_BEND);
    assert!((advance - expected).abs() < 1e-3, "{advance} vs {expected}");
    assert!(settled > before);

    rig.send(EngineCommand::PitchBend {
        deck: deck(0),
        amount: 0.0,
    });
    rig.step(BLOCK);
    let released = rig.pos(0);
    rig.step(SR as usize / 2);
    assert!((rig.pos(0) - released - f64::from(SR / 2)).abs() < 1e-3);
}

#[test]
fn cubic_interpolation_beats_linear() {
    let ratio = TempoRange::Percent10.ratio(0.8);
    let error = |interpolation| {
        let mut rig = Rig::with(EngineConfig {
            interpolation,
            ..config()
        });
        rig.load(0, sine(1000.0, 3.0));
        rig.tempo(TempoRange::Percent10, 0.8);
        rig.play(0, true);
        let out = rig.step(SR as usize);
        let mut sum = 0.0;
        let mut n = 0.0;
        for (i, frame) in out.chunks_exact(2).enumerate().skip(400) {
            let ideal = 0.5
                * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 * ratio / f64::from(SR)).sin();
            sum += (f64::from(frame[0]) - ideal).powi(2);
            n += 1.0;
        }
        (sum / n).sqrt()
    };
    let linear = error(Interpolation::Linear);
    let cubic = error(Interpolation::Cubic);
    println!("interpolation rms error: linear {linear:.2e}, cubic {cubic:.2e}");
    assert!(cubic < linear / 5.0 && cubic < 1e-3);
}

// ----- Key lock -----

#[test]
fn key_lock_keeps_pitch_at_plus_eight_percent() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 6.0));
    rig.tempo(TempoRange::Percent10, 0.8);
    rig.key_lock(true);
    rig.play(0, true);
    let out = rig.step(3 * SR as usize);

    let ratio = TempoRange::Percent10.ratio(0.8);
    let expected = f64::from(3 * SR) * ratio;
    assert!(
        (rig.pos(0) - expected).abs() < 1e-3,
        "playhead {}",
        rig.pos(0)
    );
    let settled = &out[SR as usize / 2..];
    let freq = estimate_frequency(settled);
    println!("key lock +8%: {freq:.2} Hz, rms {:.4}", rms(settled));
    assert!((freq - 1000.0).abs() < 10.0, "pitch moved to {freq} Hz");
    assert!((rms(settled) - 0.5 / 2f64.sqrt()).abs() < 0.03);
}

#[test]
fn key_lock_keeps_pitch_when_slowed_and_when_toggled_live() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 8.0));
    rig.tempo(TempoRange::Percent16, -0.75);
    rig.play(0, true);
    rig.step(SR as usize / 4);
    rig.key_lock(true);
    rig.step(SR as usize / 2);
    let out = rig.step(2 * SR as usize);
    let freq = estimate_frequency(&out);
    assert!((freq - 1000.0).abs() < 10.0, "pitch {freq} Hz");

    // And back to varispeed: pitch follows tempo again.
    rig.key_lock(false);
    rig.step(BLOCK);
    let out = rig.step(SR as usize);
    let ratio = TempoRange::Percent16.ratio(-0.75);
    let freq = estimate_frequency(&out);
    assert!((freq - 1000.0 * ratio).abs() < 10.0, "pitch {freq} Hz");
    assert!(out.iter().all(|s| s.is_finite()));
}

#[test]
fn key_lock_start_is_latency_compensated() {
    // Silence for 0.5 s, then a 1 kHz tone. Playing from 0 the tone must start
    // about 0.5 s later even though the stretcher itself delays by ~120 ms.
    let onset_frame = SR as usize / 2;
    let mut samples = vec![0.0; onset_frame * 2];
    samples.extend(sine_samples(1000.0, 3.0, 0.5));
    for (key_lock, tempo) in [(false, 0.0), (true, 0.0), (true, 0.8)] {
        let mut rig = Rig::new();
        rig.load(0, TrackBuffer::from_vec(samples.clone()).expect("track"));
        rig.tempo(TempoRange::Percent10, tempo);
        rig.key_lock(key_lock);
        rig.play(0, true);
        let out = rig.step(SR as usize);
        let rate = TempoRange::Percent10.ratio(tempo);
        let expected = onset_frame as f64 / rate;
        let envelope_onset = out
            .chunks_exact(2)
            .position(|f| f[0].abs() > 0.25)
            .expect("tone starts") as f64;
        println!(
            "onset key_lock={key_lock} tempo={tempo}: {envelope_onset} frames, expected {expected:.0}"
        );
        assert!(
            (envelope_onset - expected).abs() < f64::from(SR) * 0.015,
            "onset {envelope_onset} vs {expected}"
        );
    }
}

// ----- Transport and CUE -----

#[test]
fn pause_stops_the_playhead_and_goes_silent() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    rig.step(SR as usize / 2);
    assert!(rig.playing(0));

    rig.play(0, false);
    rig.step(BLOCK);
    assert!(!rig.playing(0));
    let parked = rig.pos(0);
    let out = rig.step(SR as usize);
    assert_eq!(rig.pos(0), parked);
    assert!(out.iter().all(|s| *s == 0.0), "silent while paused");

    // Resuming continues from the parked position.
    rig.play(0, true);
    rig.step(SR as usize / 4);
    assert!((rig.pos(0) - parked - f64::from(SR / 4)).abs() < 1e-3);
}

#[test]
fn cue_follows_cdj_behaviour() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 10.0));
    assert_eq!(rig.pos(0), 0.0);

    // Paused off the cue point: CUE sets it at the playhead.
    rig.seek(1000);
    rig.step(BLOCK);
    rig.cue(true);
    rig.step(BLOCK);
    assert!(!rig.playing(0), "setting a cue does not start playback");
    assert_eq!(
        rig.r.handle().poll_event(),
        Some(EngineEvent::CuePointSet {
            deck: deck(0),
            position: Frames(1000)
        })
    );
    rig.cue(false);
    rig.step(BLOCK);
    assert!(!rig.playing(0));

    // Paused on the cue point: hold to preview, release returns.
    rig.cue(true);
    let preview = rig.step(4800);
    assert!(rig.playing(0));
    assert!((rig.pos(0) - 5800.0).abs() < 1e-3);
    assert!(peak(&preview) > 0.4, "preview is audible");
    rig.cue(false);
    rig.step(BLOCK);
    assert!(!rig.playing(0));
    assert_eq!(rig.pos(0), 1000.0);
    let quiet = rig.step(BLOCK);
    assert!(quiet.iter().all(|s| *s == 0.0));
    assert_eq!(rig.pos(0), 1000.0);

    // Playing: CUE returns to the cue point and pauses.
    rig.play(0, true);
    rig.step(9600);
    assert!(rig.playing(0));
    rig.cue(true);
    rig.step(BLOCK);
    assert!(!rig.playing(0));
    assert_eq!(rig.pos(0), 1000.0);
    rig.cue(false);
    rig.step(BLOCK);
    assert_eq!(rig.pos(0), 1000.0);

    // PLAY while CUE is held latches playback; releasing CUE does not stop.
    rig.cue(true);
    rig.step(BLOCK);
    rig.play(0, true);
    rig.step(BLOCK);
    rig.cue(false);
    rig.step(BLOCK);
    assert!(rig.playing(0));
    assert!(rig.pos(0) > 1000.0 + 3.0 * BLOCK as f64 - 1.0);

    // A new cue point survives an explicit set.
    rig.play(0, false);
    rig.step(BLOCK);
    rig.send(EngineCommand::SetCuePoint {
        deck: deck(0),
        position: Frames(20_000),
    });
    rig.seek(20_000);
    rig.step(BLOCK);
    rig.cue(true);
    rig.step(BLOCK);
    assert!(rig.playing(0), "on the explicit cue point CUE previews");
    rig.cue(false);
    rig.step(BLOCK);
    assert_eq!(rig.pos(0), 20_000.0);
}

#[test]
fn cue_works_with_key_lock_on() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 10.0));
    rig.key_lock(true);
    rig.seek(48_000);
    rig.step(BLOCK);
    rig.cue(true);
    rig.step(BLOCK);
    rig.cue(true);
    let out = rig.step(SR as usize / 2);
    assert!(rig.playing(0));
    assert!(peak(&out) > 0.4);
    assert!((estimate_frequency(&out[2048..]) - 1000.0).abs() < 10.0);
    rig.cue(false);
    rig.step(BLOCK);
    assert_eq!(rig.pos(0), 48_000.0);
}

#[test]
fn seeking_while_playing_does_not_click() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    let mut out = rig.step(2048);
    // Half a period (24 frames) off the continuing waveform: a hard cut would
    // step by about 0.87.
    rig.seek(2048 + 24);
    out.extend(rig.step(2048));
    let max_step = out
        .chunks_exact(2)
        .zip(out.chunks_exact(2).skip(1))
        .skip(200)
        .map(|(a, b)| (b[0] - a[0]).abs())
        .fold(0.0, f32::max);
    println!("max sample step across a seek: {max_step:.3} (sine alone: 0.065)");
    assert!(max_step < 0.2, "click of {max_step}");
}

#[test]
fn track_end_stops_and_reports() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 0.1));
    rig.play(0, true);
    rig.step(SR as usize / 2);
    assert!(!rig.playing(0));
    assert_eq!(rig.pos(0), 4800.0);
    assert_eq!(
        rig.r.handle().poll_event(),
        Some(EngineEvent::TrackEnded { deck: deck(0) })
    );
    assert_eq!(rig.r.handle().poll_event(), None);
    // PLAY at the end does nothing; seeking back allows it again.
    rig.play(0, true);
    rig.step(BLOCK);
    assert!(!rig.playing(0));
    rig.seek(0);
    rig.play(0, true);
    rig.step(BLOCK);
    assert!(rig.playing(0));
}

// ----- Loading, ownership, decks, mixer -----

#[test]
fn replaced_and_ejected_buffers_come_back_for_freeing() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 1.0));
    assert_eq!(rig.r.handle().reclaim(), 0);
    rig.load(0, sine(500.0, 1.0));
    assert_eq!(rig.r.handle().reclaim(), 1, "first track returned");
    rig.send(EngineCommand::Eject { deck: deck(0) });
    rig.step(BLOCK);
    assert_eq!(rig.r.handle().reclaim(), 1, "second track returned");
    assert!(!rig.r.core().deck_status(deck(0)).expect("deck").loaded);
    // Ejecting an empty deck returns nothing and does not panic.
    rig.send(EngineCommand::Eject { deck: deck(0) });
    rig.step(BLOCK);
    assert_eq!(rig.r.handle().reclaim(), 0);
}

#[test]
fn load_resets_transport_and_sets_the_cue_point() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    rig.step(SR as usize / 4);
    rig.r
        .handle()
        .load_track(deck(0), sine(500.0, 5.0), Some(Frames(24_000)))
        .expect("room");
    rig.step(BLOCK);
    assert!(!rig.playing(0));
    assert_eq!(rig.pos(0), 0.0);
    // Cue point from the loader: moving there and pressing CUE previews.
    rig.seek(24_000);
    rig.step(BLOCK);
    rig.cue(true);
    rig.step(BLOCK);
    assert!(rig.playing(0));
}

#[test]
fn decks_are_independent_and_out_of_range_decks_are_ignored() {
    let mut rig = Rig::new();
    rig.load(1, sine(1000.0, 3.0));
    rig.play(1, true);
    let out = rig.step(SR as usize / 2);
    assert!(peak(&out) > 0.4);
    assert_eq!(rig.pos(0), 0.0);
    assert!(rig.pos(1) > 20_000.0);

    // Deck 3 is not allocated with deck_count = 2.
    rig.send(EngineCommand::SetPlaying {
        deck: deck(3),
        playing: true,
    });
    rig.send(EngineCommand::Seek {
        deck: deck(3),
        position: Frames(5),
    });
    rig.r
        .handle()
        .load_track(deck(3), sine(1000.0, 1.0), None)
        .expect("room");
    rig.step(BLOCK);
    assert!(rig.r.core().deck_status(deck(3)).is_none());
    assert_eq!(
        rig.r.handle().reclaim(),
        1,
        "buffer for a missing deck is returned"
    );
}

#[test]
fn channel_fader_trim_and_master_level_scale_the_output() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    let unity = rms(&rig.step(SR as usize / 2)[4096..]);

    rig.send(EngineCommand::SetChannelFader {
        deck: deck(0),
        position: 0.5,
    });
    rig.send(EngineCommand::SetMasterLevel { gain: Gain(0.5) });
    rig.send(EngineCommand::SetTrim {
        deck: deck(0),
        gain: Gain(2.0),
    });
    rig.step(BLOCK);
    let scaled = rms(&rig.step(SR as usize / 2)[4096..]);
    assert!(
        (scaled / unity - 0.5).abs() < 0.01,
        "ratio {}",
        scaled / unity
    );
}

#[test]
fn rejects_malformed_tracks() {
    assert_eq!(
        TrackBuffer::from_vec(vec![0.0; 5]).unwrap_err(),
        TrackError::OddSampleCount(5)
    );
}

// ----- Telemetry -----

#[test]
fn telemetry_arrives_at_about_sixty_hertz() {
    let mut rig = Rig::new();
    rig.load(0, sine(1000.0, 5.0));
    rig.play(0, true);
    rig.r.handle().poll_telemetry();

    let mut sequences = Vec::new();
    let mut last = None;
    for _ in 0..(SR as usize / BLOCK) {
        rig.step(BLOCK);
        if let Some(t) = rig.r.handle().poll_telemetry() {
            sequences.push(t.sequence);
            last = Some(t);
        }
    }
    let snapshots = sequences.len();
    println!("snapshots in ~1 s: {snapshots}");
    assert!((57..=63).contains(&snapshots), "{snapshots} snapshots");
    assert!(sequences.windows(2).all(|w| w[1] > w[0]));

    let t = last.expect("telemetry");
    let d = t.decks[0];
    assert_eq!(t.sample_rate, SampleRate(SR));
    assert!(d.loaded && d.playing);
    assert_eq!(d.duration, Frames(5 * u64::from(SR)));
    assert!(d.position.0 > 40_000 && d.position.0 <= 48_000 + 2 * BLOCK as u64);
    assert!((d.rate - 1.0).abs() < 1e-9);
    assert!(d.meter.peak_l > 0.45 && d.meter.peak_l <= 0.51);
    assert!((f64::from(d.meter.rms_l) - 0.5 / 2f64.sqrt()).abs() < 0.02);
    assert!(t.master.peak_l > 0.45);
    assert!(!t.decks[1].loaded);
    assert!(t.dsp_load >= 0.0 && t.dsp_load.is_finite());
    assert_eq!(t.engine_time.0 % BLOCK as u64, 0);
}

// ----- Real-time safety -----

/// A scripted mix of everything the callback can do, under the allocation
/// guard: any allocation or free aborts the test process.
#[test]
fn every_deck_path_is_allocation_free() {
    let mut rig = Rig::new();
    let tracks = [sine(1000.0, 4.0), sine(220.0, 3.0)];
    let mut seed = 0x2545_F491_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut buffer = vec![0.0; 777 * 2];
    for round in 0..400u32 {
        let track = &tracks[(round as usize / 50) % 2];
        match next() % 12 {
            0 => {
                let _ = rig.r.handle().load_track(deck(0), track.clone(), None);
            }
            1 => rig.play(0, next() % 2 == 0),
            2 => rig.cue(next() % 2 == 0),
            3 => rig.seek(u64::from(next() % 150_000)),
            4 => rig.key_lock(next() % 2 == 0),
            5 => rig.tempo(
                TempoRange::CYCLE[(next() % 4) as usize],
                (next() % 2001) as f32 / 1000.0 - 1.0,
            ),
            6 => rig.send(EngineCommand::PitchBend {
                deck: deck(0),
                amount: (next() % 201) as f32 / 100.0 - 1.0,
            }),
            7 => rig.send(EngineCommand::Eject { deck: deck(0) }),
            8 => rig.send(EngineCommand::SetPlaying {
                deck: deck(1),
                playing: true,
            }),
            9 => rig.send(EngineCommand::SetTempoFader {
                deck: deck(0),
                position: f32::NAN,
            }),
            _ => {}
        }
        rig.r.render_into(&mut buffer);
        assert!(buffer.iter().all(|s| s.is_finite() && s.abs() < 4.0));
        rig.r.handle().reclaim();
        rig.r.handle().poll_telemetry();
        while rig.r.handle().poll_event().is_some() {}
    }
}

/// The guard really aborts on allocation: re-run this binary with a test that
/// allocates inside `rt_guard` and expect it to die.
#[cfg(debug_assertions)]
#[test]
fn guard_trips_on_allocation() {
    let exe = std::env::current_exe().expect("test binary path");
    let status = std::process::Command::new(exe)
        .args([
            "--exact",
            "tests_playback::allocation_child",
            "--test-threads=1",
        ])
        .env("OPENDECK_ALLOC_CHILD", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("spawn child");
    assert!(!status.success(), "allocating under the guard must abort");
}

/// Body of the child process for [`guard_trips_on_allocation`]. A no-op in a
/// normal run.
#[cfg(debug_assertions)]
#[test]
fn allocation_child() {
    if std::env::var_os("OPENDECK_ALLOC_CHILD").is_some() {
        crate::rt_guard(|| {
            let v = vec![1u8; 64];
            std::hint::black_box(&v);
        });
    }
}

// ----- CPU -----

#[test]
fn sixty_seconds_render_cost() {
    let seconds = 60;
    for (label, key_lock, tempo) in [("varispeed +8%", false, 0.8), ("key lock +8%", true, 0.8)] {
        let mut rig = Rig::new();
        rig.load(0, sine(1000.0, f64::from(seconds) + 5.0));
        rig.tempo(TempoRange::Percent10, tempo);
        rig.key_lock(key_lock);
        rig.play(0, true);
        let mut out = vec![0.0; seconds as usize * SR as usize * 2];
        rig.r.render_into(&mut out);
        let stats = rig.r.stats();
        let budget = BLOCK as f64 / f64::from(SR);
        println!(
            "{label}: {} blocks of {BLOCK} frames, mean {:?} per block, worst {:?}, budget {:.2} ms, {:.2}% of realtime (1 deck playing, deck 2 idle)",
            stats.blocks,
            stats.mean_block(),
            stats.worst_block,
            budget * 1000.0,
            stats.realtime_fraction(SR) * 100.0
        );
        assert!(out.iter().all(|s| s.is_finite()));
        if !cfg!(debug_assertions) {
            assert!(stats.realtime_fraction(SR) < 0.25, "{label} too slow");
        }
    }
}
