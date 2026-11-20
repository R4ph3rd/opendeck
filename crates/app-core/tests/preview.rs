//! Track preview through `AudioController`, headless on the offline backend:
//! the loader thread decodes, the engine plays the preview voice on the
//! headphone bus (or the master without a headphone pair).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreError, CoreEvent, LaunchOptions, SettingsStore,
};
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::settings::AudioSettings;
use opendeck_contracts::{EngineTelemetry, SampleRate};

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-preview-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// 16-bit stereo sine at the engine rate, `amp` of full scale.
fn sine_wav(path: &Path, freq: f64, secs: f64, amp: f64) {
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
        let v = amp * (std::f64::consts::TAU * freq * i as f64 / f64::from(RATE)).sin();
        let q = (v * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn launch(settings: AudioSettings) -> (AppCore, AudioController, Receiver<CoreEvent>) {
    let store = SettingsStore::memory();
    let core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        store,
        max_track_secs: 60.0,
        ..LaunchOptions::default()
    })
    .expect("launch");
    let audio = core.audio().expect("audio");
    audio.set_audio_settings(settings).expect("settings");
    let events = audio.subscribe().expect("subscribe");
    (core, audio, events)
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

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0, |m, s| m.max(s.abs()))
}

/// Renders in 20 ms steps until the headphone or master bus carries sound.
fn render_until_sound(audio: &AudioController) -> (f32, f32) {
    let deadline = Instant::now() + WAIT;
    loop {
        let out = audio.render_offline(RATE as usize / 50).expect("render");
        let (m, h) = (peak(&out.master), peak(&out.headphones));
        if m > 0.01 || h > 0.01 {
            return (m, h);
        }
        assert!(Instant::now() < deadline, "the preview never sounded");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_preview_decodes_on_the_loader_and_plays_on_the_headphones() {
    let dir = tmp_dir("phones");
    let a = dir.join("a.wav");
    sine_wav(&a, 440.0, 3.0, 0.5);
    let (_core, audio, events) = launch(AudioSettings::default());

    let token = audio
        .preview_play(TrackId(1), &a, 3.0, 0.1)
        .expect("preview");
    assert_ne!(token, 0);
    let (master, phones) = render_until_sound(&audio);
    assert!(master < 1e-6, "master stays silent, got {master}");
    assert!((phones - 0.5).abs() < 0.02, "headphones {phones}");

    let t = wait_telemetry(&events, |t| t.preview.token == token && t.preview.playing);
    assert!(t.preview.loaded);
    assert_eq!(t.preview.duration.0, 3 * u64::from(RATE));
    assert!(t.preview.position.0 >= u64::from(RATE) * 3 / 10);

    // Same track: no new decode, same token, the playhead jumps.
    let again = audio
        .preview_play(TrackId(1), &a, 3.0, 0.9)
        .expect("preview again");
    assert_eq!(again, token);
    let t = wait_telemetry(&events, |t| {
        t.preview.position.0 >= u64::from(RATE) * 27 / 10
    });
    assert_eq!(t.preview.token, token);

    audio.preview_stop().expect("stop");
    let t = wait_telemetry(&events, |t| !t.preview.playing);
    assert!(t.preview.loaded, "the buffer stays for a replay");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn another_track_takes_over_with_a_new_token() {
    let dir = tmp_dir("switch");
    let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
    sine_wav(&a, 440.0, 3.0, 0.5);
    sine_wav(&b, 880.0, 5.0, 0.25);
    let (_core, audio, events) = launch(AudioSettings::default());

    let first = audio.preview_play(TrackId(1), &a, 3.0, 0.0).expect("a");
    wait_telemetry(&events, |t| t.preview.token == first && t.preview.playing);
    let second = audio.preview_play(TrackId(2), &b, 5.0, 0.0).expect("b");
    assert_ne!(first, second);
    let t = wait_telemetry(&events, |t| t.preview.token == second && t.preview.playing);
    assert_eq!(t.preview.duration.0, 5 * u64::from(RATE));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_headphone_pair_the_preview_plays_on_the_master_at_minus_12_db() {
    let dir = tmp_dir("master");
    let a = dir.join("a.wav");
    sine_wav(&a, 440.0, 3.0, 0.5);
    let (_core, audio, _events) = launch(AudioSettings {
        headphones: None,
        ..AudioSettings::default()
    });

    audio.preview_play(TrackId(1), &a, 3.0, 0.0).expect("play");
    let (master, phones) = render_until_sound(&audio);
    let expected = 0.5 * opendeck_engine::PREVIEW_MASTER_GAIN;
    assert!((master - expected).abs() < 0.01, "master {master}");
    assert!(phones < 1e-6, "headphones {phones}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_undecodable_file_reports_a_failure_and_a_long_track_is_refused() {
    let dir = tmp_dir("fail");
    let bad = dir.join("bad.wav");
    std::fs::write(&bad, b"not audio at all").expect("write");
    let (_core, audio, events) = launch(AudioSettings::default());

    audio
        .preview_play(TrackId(9), &bad, 3.0, 0.0)
        .expect("queue");
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(CoreEvent::PreviewFailed(f)) => {
                assert_eq!(f.track, TrackId(9));
                assert!(!f.message.is_empty());
                break;
            }
            Ok(_) => {}
            Err(_) => panic!("no preview-failed event"),
        }
    }

    let err = audio
        .preview_play(TrackId(9), &bad, 4.0 * 3600.0, 0.0)
        .expect_err("too long");
    assert!(matches!(err, CoreError::TooLong { .. }), "{err:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restarting_the_engine_ends_the_preview() {
    let dir = tmp_dir("restart");
    let a = dir.join("a.wav");
    sine_wav(&a, 440.0, 3.0, 0.5);
    let (_core, audio, events) = launch(AudioSettings::default());

    let token = audio.preview_play(TrackId(1), &a, 3.0, 0.0).expect("play");
    wait_telemetry(&events, |t| t.preview.token == token && t.preview.playing);
    audio
        .set_audio_settings(AudioSettings {
            sample_rate: Some(44_100),
            ..AudioSettings::default()
        })
        .expect("restart");
    let t = wait_telemetry(&events, |t| t.sample_rate == SampleRate(44_100));
    assert!(!t.preview.loaded && !t.preview.playing);
    // The same track decodes again at the new rate and gets a new token.
    let again = audio.preview_play(TrackId(1), &a, 3.0, 0.0).expect("again");
    assert_ne!(again, token);
    wait_telemetry(&events, |t| t.preview.token == again && t.preview.playing);
    let _ = std::fs::remove_dir_all(&dir);
}
