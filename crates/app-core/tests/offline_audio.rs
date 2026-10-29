//! Headless audio tests: commands sent through `AudioController` change what
//! the engine renders. The control thread drives an `OfflineRenderer`;
//! `render_offline` returns master and headphone audio for inspection.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreEvent, LaunchOptions, OfflineAudio, SettingsStore,
};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::HotCueIndex;
use opendeck_contracts::mixer::{CrossfaderAssign, EqBand};
use opendeck_contracts::settings::AudioSettings;
use opendeck_contracts::tempo::SyncMode;
use opendeck_contracts::{Bpm, DeckId, EngineTelemetry, Gain, Seconds};

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-audio-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Writes a 16-bit stereo PCM WAV at the engine rate; `sample(i)` is frame `i`.
fn write_wav(path: &Path, secs: f64, sample: impl Fn(usize) -> f64) {
    let frames = (f64::from(RATE) * secs) as usize;
    let data_len = (frames * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + frames * 4);
    bytes.extend(b"RIFF");
    bytes.extend((36 + data_len).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(RATE.to_le_bytes());
    bytes.extend((RATE * 4).to_le_bytes());
    bytes.extend(4u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_len.to_le_bytes());
    for i in 0..frames {
        let q = (sample(i).clamp(-1.0, 1.0) * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn sine_wav(path: &Path, freq: f64, secs: f64) {
    write_wav(path, secs, |i| {
        0.5 * (std::f64::consts::TAU * freq * i as f64 / f64::from(RATE)).sin()
    });
}

/// A 2 ms decaying 1 kHz burst on every beat, the first at `offset` seconds.
fn click_wav(path: &Path, bpm: f64, offset: f64, secs: f64) {
    let beat = 60.0 / bpm * f64::from(RATE);
    let first = offset * f64::from(RATE);
    write_wav(path, secs, |i| {
        let t = i as f64 - first;
        if t < 0.0 {
            return 0.0;
        }
        let k = t - (t / beat).floor() * beat;
        if k >= 96.0 {
            return 0.0;
        }
        0.8 * (-k / 16.0).exp() * (std::f64::consts::TAU * 1000.0 * k / f64::from(RATE)).sin()
    });
}

/// Deterministic white noise: never periodic over a test's span.
fn noise_wav(path: &Path, secs: f64) {
    write_wav(path, secs, |i| {
        let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
        x ^= x >> 31;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 29;
        (x >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    });
}

fn launch() -> (AudioController, Receiver<CoreEvent>) {
    let core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        store: SettingsStore::memory(),
        ..LaunchOptions::default()
    })
    .expect("launch");
    let audio = core.audio().expect("audio");
    let events = audio.subscribe().expect("subscribe");
    (audio, events)
}

fn deck(index: u8) -> DeckId {
    DeckId::new(index).expect("deck")
}

fn wait_telemetry(
    events: &Receiver<CoreEvent>,
    ok: impl Fn(&EngineTelemetry) -> bool,
) -> EngineTelemetry {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(CoreEvent::Telemetry(t)) if ok(&t) => return *t,
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for telemetry"),
        }
    }
}

fn load(
    audio: &AudioController,
    events: &Receiver<CoreEvent>,
    deck: DeckId,
    path: &Path,
    grid: Option<BeatGrid>,
) {
    audio
        .load_track_with_grid(deck, path, grid)
        .expect("load_track_with_grid");
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(CoreEvent::TrackLoaded(t)) if t.deck == deck => return,
            Ok(CoreEvent::TrackLoadFailed(f)) => panic!("load failed: {}", f.message),
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for the load"),
        }
    }
}

fn render(audio: &AudioController, secs: f64) -> OfflineAudio {
    audio
        .render_offline((secs * f64::from(RATE)) as usize)
        .expect("offline render")
}

fn left(stereo: &[f32]) -> Vec<f32> {
    stereo.chunks_exact(2).map(|f| f[0]).collect()
}

fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s).powi(2)).sum();
    (sum / samples.len().max(1) as f64).sqrt()
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.max(1e-12).log10()
}

/// Amplitude of `freq` in the left channel (Goertzel).
fn tone_level(stereo: &[f32], freq: f64) -> f64 {
    let x = left(stereo);
    let w = std::f64::consts::TAU * freq / f64::from(RATE);
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0, 0.0);
    for v in &x {
        let s0 = f64::from(*v) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    2.0 * power.max(0.0).sqrt() / x.len().max(1) as f64
}

/// Frames where a click starts: the first sample at or above `0.05` after at
/// least 10 ms of quiet, with a peak above `0.2` within the next 1 ms.
fn onsets(stereo: &[f32]) -> Vec<usize> {
    let x = left(stereo);
    let mut out = Vec::new();
    let mut quiet = usize::MAX / 2;
    for (i, v) in x.iter().enumerate() {
        if v.abs() < 0.05 {
            quiet += 1;
            continue;
        }
        if quiet >= 480 && x[i..].iter().take(48).any(|s| s.abs() > 0.2) {
            out.push(i);
        }
        quiet = 0;
    }
    out
}

#[test]
fn eq_low_kill_lowers_a_low_sine() {
    let dir = tmp_dir("eq");
    let wav = dir.join("low.wav");
    sine_wav(&wav, 60.0, 10.0);
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &wav, None);
    audio.set_playing(deck(0), true).expect("play");
    render(&audio, 0.3);
    let before = rms(&render(&audio, 0.5).master);

    audio
        .set_eq(deck(0), EqBand::Low, Gain::SILENCE)
        .expect("kill");
    render(&audio, 0.3);
    let after = rms(&render(&audio, 0.5).master);
    let drop = db(after / before);
    println!("60 Hz: {before:.4} rms, killed {after:.2e} rms ({drop:.1} dB)");
    assert!(before > 0.2, "{before}");
    assert!(drop < -40.0, "low kill only {drop:.1} dB");

    audio
        .set_eq(deck(0), EqBand::Low, Gain::UNITY)
        .expect("unity");
    render(&audio, 0.3);
    let back = rms(&render(&audio, 0.5).master);
    assert!((db(back / before)).abs() < 0.5, "{back} vs {before}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn crossfader_full_a_silences_deck_b() {
    let dir = tmp_dir("xfader");
    let (wav_a, wav_b) = (dir.join("a.wav"), dir.join("b.wav"));
    sine_wav(&wav_a, 220.0, 10.0);
    sine_wav(&wav_b, 660.0, 10.0);
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &wav_a, None);
    load(&audio, &events, deck(1), &wav_b, None);
    audio
        .set_crossfader_assign(deck(0), CrossfaderAssign::A)
        .expect("assign A");
    audio
        .set_crossfader_assign(deck(1), CrossfaderAssign::B)
        .expect("assign B");
    audio.set_playing(deck(0), true).expect("play A");
    audio.set_playing(deck(1), true).expect("play B");

    audio.set_crossfader(-1.0).expect("full A");
    render(&audio, 0.2);
    let full_a = render(&audio, 0.5).master;
    let (a, b) = (tone_level(&full_a, 220.0), tone_level(&full_a, 660.0));
    println!("full A: deck A {a:.4}, deck B {b:.2e}");
    assert!(a > 0.4, "deck A level {a}");
    assert!(b < 1e-4, "deck B leaks at {b}");

    audio.set_crossfader(1.0).expect("full B");
    render(&audio, 0.2);
    let full_b = render(&audio, 0.5).master;
    let (a, b) = (tone_level(&full_b, 220.0), tone_level(&full_b, 660.0));
    println!("full B: deck A {a:.2e}, deck B {b:.4}");
    assert!(b > 0.4 && a < 1e-4, "{a} {b}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pfl_routes_a_deck_to_the_headphones_only() {
    let dir = tmp_dir("pfl");
    let wav = dir.join("tone.wav");
    sine_wav(&wav, 440.0, 10.0);
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &wav, None);
    audio.set_channel_fader(deck(0), 0.0).expect("fader down");
    audio.set_headphone_mix(0.0).expect("cue only");
    audio.set_pfl(deck(0), true).expect("pfl");
    audio.set_playing(deck(0), true).expect("play");
    render(&audio, 0.2);
    let cued = render(&audio, 0.5);
    let (master, phones) = (rms(&cued.master), rms(&cued.headphones));
    println!("PFL on, fader down: master {master:.2e}, headphones {phones:.4}");
    assert!(master < 1e-6, "master {master}");
    assert!(phones > 0.1, "headphones {phones}");

    audio.set_pfl(deck(0), false).expect("pfl off");
    render(&audio, 0.2);
    let off = render(&audio, 0.5);
    assert!(rms(&off.headphones) < 1e-6, "{}", rms(&off.headphones));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_loop_repeats_the_same_audio() {
    let dir = tmp_dir("loop");
    let wav = dir.join("noise.wav");
    noise_wav(&wav, 20.0);
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &wav, None);
    audio.set_playing(deck(0), true).expect("play");
    render(&audio, 0.5);
    audio.loop_in(deck(0)).expect("loop in");
    render(&audio, 0.75);
    audio.loop_out(deck(0)).expect("loop out");
    render(&audio, 0.1);

    let t = wait_telemetry(&events, |t| t.decks[0].loop_active);
    let region = t.decks[0].loop_region.expect("loop region");
    let len = (region.end.expect("loop end").0 - region.start.0) as usize;
    let secs = len as f64 / f64::from(RATE);
    println!("loop of {len} frames ({secs:.3} s)");
    assert!((secs - 0.75).abs() < 0.05, "{secs}");

    let out = left(&render(&audio, 3.0 * secs).master);
    assert!(rms(&out) > 0.1, "silent loop");
    let worst = (0..2 * len)
        .map(|i| (out[i] - out[i + len]).abs())
        .fold(0.0_f32, f32::max);
    println!("loop repeats: worst difference {worst:e}");
    assert!(worst < 1e-5, "{worst}");
    // Noise is not periodic by itself: without the loop this would fail.
    let shifted = (0..len)
        .map(|i| (out[i] - out[i + len / 2]).abs())
        .fold(0.0_f32, f32::max);
    assert!(shifted > 0.1, "{shifted}");

    audio.loop_toggle(deck(0)).expect("exit");
    let t = wait_telemetry(&events, |t| !t.decks[0].loop_active);
    assert_eq!(t.decks[0].loop_region, Some(region));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn beat_sync_aligns_two_click_tracks() {
    let dir = tmp_dir("sync");
    let (wav_a, wav_b) = (dir.join("a.wav"), dir.join("b.wav"));
    click_wav(&wav_a, 120.0, 0.0, 30.0);
    click_wav(&wav_b, 126.0, 0.1, 30.0);
    let grid_a = BeatGrid::new(Seconds(0.0), Bpm(120.0), 4).expect("grid A");
    let grid_b = BeatGrid::new(Seconds(0.1), Bpm(126.0), 4).expect("grid B");
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &wav_a, Some(grid_a));
    load(&audio, &events, deck(1), &wav_b, Some(grid_b));
    // Deck A on the master only, deck B on the headphones only.
    audio.set_headphone_mix(0.0).expect("cue only");
    audio.set_channel_fader(deck(1), 0.0).expect("B fader down");
    audio.set_pfl(deck(1), true).expect("B pfl");

    audio.set_playing(deck(0), true).expect("play A");
    render(&audio, 0.7);
    audio.set_playing(deck(1), true).expect("play B");
    render(&audio, 1.0);
    let free = render(&audio, 2.0);
    let free_b = onsets(&free.headphones);
    let free_gap = (free_b[2] - free_b[1]) as f64 / f64::from(RATE);
    assert!((free_gap - 60.0 / 126.0).abs() < 0.002, "{free_gap}");

    audio.set_sync(deck(1), SyncMode::Beat).expect("sync B");
    let t = wait_telemetry(&events, |t| t.decks[1].sync_mode == SyncMode::Beat);
    assert!(t.decks[0].is_leader, "deck A leads");
    render(&audio, 1.0);
    let synced = render(&audio, 3.0);
    let (a, b) = (onsets(&synced.master), onsets(&synced.headphones));
    assert!(a.len() >= 5 && b.len() >= 5, "{a:?} {b:?}");
    let gap = (b[3] - b[2]) as f64 / f64::from(RATE);
    let worst = b
        .iter()
        .map(|&ob| {
            a.iter()
                .map(|&oa| (ob as f64 - oa as f64).abs())
                .fold(f64::MAX, f64::min)
        })
        .fold(0.0, f64::max)
        / f64::from(RATE)
        * 1000.0;
    println!("synced: B beat {gap:.4} s, worst onset offset {worst:.3} ms");
    assert!((gap - 0.5).abs() < 0.001, "{gap}");
    assert!(worst < 1.0, "beats {worst:.3} ms apart");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hot_cues_and_the_grid_are_resent_after_an_engine_restart() {
    let dir = tmp_dir("restart");
    let wav = dir.join("noise.wav");
    noise_wav(&wav, 20.0);
    let grid = BeatGrid::new(Seconds(0.0), Bpm(120.0), 4).expect("grid");
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &wav, Some(grid));
    audio.set_quantize(deck(0), false).expect("quantize off");
    audio.seek(deck(0), Seconds(1.0)).expect("seek");
    render(&audio, 0.01);
    wait_telemetry(&events, |t| t.decks[0].position.0 >= u64::from(RATE));
    let pad = HotCueIndex::new(0).expect("pad");
    let cue = audio.set_hot_cue(deck(0), pad).expect("set hot cue");

    // A restart reloads the track; the engine starts it without cues.
    audio
        .set_audio_settings(AudioSettings {
            buffer_frames: Some(256),
            ..AudioSettings::default()
        })
        .expect("restart");
    let deadline = Instant::now() + WAIT;
    while !matches!(
        events.recv_timeout(deadline.saturating_duration_since(Instant::now())),
        Ok(CoreEvent::TrackLoaded(_))
    ) {
        assert!(Instant::now() < deadline, "no reload");
    }
    let state = audio.deck_state(deck(0)).expect("deck state");
    assert_eq!(state.beat_grid, Some(grid));
    assert_eq!(state.track_bpm, Some(Bpm(120.0)));

    audio.set_playing(deck(0), true).expect("play");
    render(&audio, 2.0);
    wait_telemetry(&events, |t| t.decks[0].position.0 >= 2 * u64::from(RATE));
    audio.trigger_hot_cue(deck(0), pad, true).expect("press");
    audio.trigger_hot_cue(deck(0), pad, false).expect("release");
    render(&audio, 0.01);
    let t = wait_telemetry(&events, |t| {
        t.decks[0].position.0 < 2 * u64::from(RATE) && t.decks[0].playing
    });
    let at = t.decks[0].position.0 as f64 / f64::from(RATE);
    println!(
        "hot cue at {:.3} s, playhead after the jump {at:.3} s",
        cue.start.0
    );
    assert!(at >= cue.start.0 && at < cue.start.0 + 0.5, "{at}");
    let t = wait_telemetry(&events, |t| t.decks[0].bpm.is_some());
    assert!((t.decks[0].bpm.expect("bpm").0 - 120.0).abs() < 1e-6);
    let _ = std::fs::remove_dir_all(&dir);
}
