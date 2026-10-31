//! Engine restarts (settings change, device loss) keep deck state: the
//! playhead, the play state and the mixer come back. Headless: the real
//! control and loader threads around an `OfflineRenderer`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreEvent, LaunchOptions, SettingsStore,
};
use opendeck_contracts::mixer::EqBand;
use opendeck_contracts::settings::{
    AudioHostKind, AudioSettings, ChannelPair, InputChannels, TalkOverSettings,
};
use opendeck_contracts::{DeckId, EngineTelemetry, Gain, Seconds};

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-restart-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// 16-bit stereo PCM WAV with a 220 Hz sine.
fn write_wav(path: &Path, rate: u32, secs: f32) {
    let frames = (rate as f32 * secs) as usize;
    let data_len = (frames * 4) as u32;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + data_len).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * 4).to_le_bytes());
    bytes.extend(4u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_len.to_le_bytes());
    for i in 0..frames {
        let s = (std::f64::consts::TAU * 220.0 * i as f64 / f64::from(rate)).sin() * 0.5;
        let q = (s * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
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

fn deck_a() -> DeckId {
    DeckId::new(0).expect("deck A")
}

fn wait_for<T>(events: &Receiver<CoreEvent>, mut pick: impl FnMut(&CoreEvent) -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(event) => {
                if let Some(found) = pick(&event) {
                    return found;
                }
            }
            Err(_) => panic!("timed out waiting for an event"),
        }
    }
}

fn wait_telemetry(
    events: &Receiver<CoreEvent>,
    ok: impl Fn(&EngineTelemetry) -> bool,
) -> EngineTelemetry {
    wait_for(events, |e| match e {
        CoreEvent::Telemetry(t) if ok(t) => Some(**t),
        _ => None,
    })
}

fn load_and_wait(audio: &AudioController, events: &Receiver<CoreEvent>, path: &Path) {
    audio.load_track(deck_a(), path).expect("load_track");
    wait_for(events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
}

fn seconds(t: &EngineTelemetry, deck: usize) -> f64 {
    t.decks[deck].position.0 as f64 / f64::from(t.sample_rate.0)
}

#[test]
fn a_playing_deck_resumes_where_it_was_after_a_sample_rate_change() {
    let dir = tmp_dir("rate");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 20.0);
    let (audio, events) = launch();
    load_and_wait(&audio, &events, &wav);
    audio.seek(deck_a(), Seconds(5.0)).expect("seek");
    audio.set_playing(deck_a(), true).expect("play");
    let before = wait_telemetry(&events, |t| {
        t.decks[0].playing && seconds(t, 0) > 5.3 && seconds(t, 0) < 6.0
    });
    let at = seconds(&before, 0);

    let status = audio
        .set_audio_settings(AudioSettings {
            sample_rate: Some(44_100),
            ..AudioSettings::default()
        })
        .expect("restart at 44.1 kHz");
    assert_eq!(status.sample_rate, Some(44_100));

    // The track reloads at the new rate; the deck is playing near where it was.
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    let after = wait_telemetry(&events, |t| {
        t.sample_rate.0 == 44_100 && t.decks[0].playing && t.decks[0].loaded
    });
    let resumed = seconds(&after, 0);
    assert!(
        resumed >= at && resumed < at + 3.0,
        "was at {at:.2} s, resumed at {resumed:.2} s"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_paused_deck_stays_paused_at_its_position_after_a_buffer_change() {
    let dir = tmp_dir("buffer");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 20.0);
    let (audio, events) = launch();
    load_and_wait(&audio, &events, &wav);
    audio.seek(deck_a(), Seconds(7.5)).expect("seek");
    wait_telemetry(&events, |t| (seconds(t, 0) - 7.5).abs() < 0.01);

    audio
        .set_audio_settings(AudioSettings {
            buffer_frames: Some(256),
            ..AudioSettings::default()
        })
        .expect("restart with a new buffer size");
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    let after = wait_telemetry(&events, |t| t.decks[0].loaded && seconds(t, 0) > 7.0);
    assert!(!after.decks[0].playing, "a paused deck stays paused");
    assert!(
        (seconds(&after, 0) - 7.5).abs() < 0.05,
        "position {:.3}",
        seconds(&after, 0)
    );
    assert_eq!(audio.status().expect("status").buffer_frames, Some(256));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_main_cue_point_survives_a_restart() {
    let dir = tmp_dir("cue");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 20.0);
    let (audio, events) = launch();
    load_and_wait(&audio, &events, &wav);
    audio.seek(deck_a(), Seconds(4.0)).expect("seek");
    wait_telemetry(&events, |t| (seconds(t, 0) - 4.0).abs() < 0.01);
    audio.cue(deck_a(), true).expect("cue down");
    audio.cue(deck_a(), false).expect("cue up");
    // Let the engine report the new cue point.
    wait_for(&events, |e| {
        matches!(
            e,
            CoreEvent::Engine(opendeck_contracts::EngineEvent::CuePointSet { .. })
        )
        .then_some(())
    });
    audio.seek(deck_a(), Seconds(9.0)).expect("seek away");
    wait_telemetry(&events, |t| (seconds(t, 0) - 9.0).abs() < 0.01);

    audio
        .set_audio_settings(AudioSettings {
            sample_rate: Some(44_100),
            ..AudioSettings::default()
        })
        .expect("restart");
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    wait_telemetry(&events, |t| t.decks[0].loaded && seconds(t, 0) > 8.0);
    // CUE while playing returns to the stored cue point, not to the start.
    audio.set_playing(deck_a(), true).expect("play");
    wait_telemetry(&events, |t| t.decks[0].playing);
    audio.cue(deck_a(), true).expect("cue down");
    let at_cue = wait_telemetry(&events, |t| !t.decks[0].playing && seconds(t, 0) < 5.0);
    assert!(
        (seconds(&at_cue, 0) - 4.0).abs() < 0.1,
        "{}",
        seconds(&at_cue, 0)
    );
    audio.cue(deck_a(), false).expect("cue up");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn booth_mic_aux_and_talk_over_survive_a_restart() {
    let (audio, _events) = launch();
    let talk = TalkOverSettings {
        enabled: true,
        depth_db: 9.0,
        threshold_db: -30.0,
    };
    audio.set_booth_level(Gain(0.5)).expect("booth");
    audio.set_mic_level(Gain(0.7)).expect("mic level");
    audio.set_mic_eq(EqBand::Low, Gain(0.4)).expect("mic low");
    audio.set_mic_eq(EqBand::High, Gain(1.5)).expect("mic high");
    audio.set_mic_on(false).expect("mic on");
    audio.set_aux_level(Gain(0.8)).expect("aux level");
    audio.set_aux_fader(0.25).expect("aux fader");
    audio.set_talk_over(talk).expect("talk-over");
    let expected = audio.input_state().expect("input state");
    assert_eq!(expected.booth_level, Gain(0.5));
    assert_eq!(expected.mic_eq_low, Gain(0.4));
    assert!(!expected.mic_on);
    assert_eq!(expected.aux_fader, 0.25);
    assert_eq!(expected.talk_over, talk);

    // Invalid values are refused before they reach the engine.
    assert!(audio.set_booth_level(Gain(f32::NAN)).is_err());
    assert!(audio.set_mic_eq(EqBand::Mid, Gain(1.0)).is_err());
    assert!(audio.set_aux_fader(f32::INFINITY).is_err());

    // A restart that keeps the booth route replays all of it.
    let mut settings = audio.audio_settings().expect("settings");
    assert_eq!(
        settings.booth_level, 0.5,
        "booth level mirrors into settings"
    );
    assert_eq!(settings.talkover, talk);
    settings.booth = Some(ChannelPair { left: 4, right: 5 });
    settings.booth_mono = true;
    audio.set_audio_settings(settings).expect("restart");
    assert_eq!(audio.input_state().expect("state"), expected);
    let status = audio.status().expect("status");
    assert_eq!(status.booth, Some(ChannelPair { left: 4, right: 5 }));
    assert!(status.booth_mono);
}

#[test]
fn the_booth_bus_follows_the_master_at_its_own_level() {
    let dir = tmp_dir("booth");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 10.0);
    let (audio, events) = launch();
    load_and_wait(&audio, &events, &wav);
    audio.set_playing(deck_a(), true).expect("play");
    audio.set_booth_level(Gain(0.5)).expect("booth level");
    let out = audio.render_offline(RATE as usize).expect("render");
    let tail = RATE as usize;
    for (m, b) in out.master[tail..].iter().zip(&out.booth[tail..]) {
        assert!((m * 0.5 - b).abs() < 1e-5, "booth is master at half level");
    }
    assert!(out.master.iter().any(|s| s.abs() > 0.1));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn input_settings_are_stored_and_reported_even_without_a_device() {
    let (audio, _events) = launch();
    let settings = AudioSettings {
        mic: Some(InputChannels::mono(2)),
        aux: Some(ChannelPair { left: 0, right: 1 }),
        input_device: Some("DDJ-FLX4".to_owned()),
        ..AudioSettings::default()
    };
    let status = audio.set_audio_settings(settings.clone()).expect("apply");
    // The offline backend opens no input and says so.
    assert!(status.input_error.is_some());
    assert_eq!(audio.audio_settings().expect("settings"), settings);
}

#[test]
fn hosts_list_capability_flags_and_platform_hosts() {
    let core = AppCore::new();
    let hosts = core.audio_hosts();
    assert!(!hosts.is_empty());
    assert_eq!(hosts.iter().filter(|h| h.is_default).count(), 1);
    // Every host this platform can offer is listed, available or not.
    let expect_jack = cfg!(any(target_os = "linux", target_os = "macos"));
    assert_eq!(
        hosts.iter().any(|h| h.kind == AudioHostKind::Jack),
        expect_jack
    );
    for host in &hosts {
        if !host.available {
            assert!(!host.input && !host.multichannel_output && !host.configurable_buffer);
        }
    }
    let _ = core.audio_input_devices(None);
}
