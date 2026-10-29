//! Headless integration tests: the real control and loader threads around an
//! `OfflineRenderer` (no audio device).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreError, CoreEvent, LaunchOptions, SettingsStore,
};
use opendeck_contracts::cue::{CueKind, HotCueIndex};
use opendeck_contracts::mixer::{CrossfaderAssign, CrossfaderCurve, EqBand};
use opendeck_contracts::settings::{AudioSettings, ChannelPair};
use opendeck_contracts::tempo::SyncMode;
use opendeck_contracts::{Beats, DeckId, EngineTelemetry, Gain, Seconds};

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-core-{}-{name}", std::process::id()));
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

fn launch(store: SettingsStore) -> (AudioController, Receiver<CoreEvent>) {
    let core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        store,
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

/// Waits for the first event `pick` accepts, ignoring the rest.
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

/// Waits for telemetry satisfying `ok` and returns it.
fn wait_telemetry(
    events: &Receiver<CoreEvent>,
    ok: impl Fn(&EngineTelemetry) -> bool,
) -> EngineTelemetry {
    wait_for(events, |e| match e {
        CoreEvent::Telemetry(t) if ok(t) => Some(**t),
        _ => None,
    })
}

fn load(audio: &AudioController, events: &Receiver<CoreEvent>, path: &Path) {
    audio.load_track(deck_a(), path).expect("load_track");
    wait_for(events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
}

#[test]
fn load_play_pause_advances_the_playhead() {
    let dir = tmp_dir("play");
    let wav = dir.join("tone.wav");
    // 44.1 kHz file into a 48 kHz engine: exercises the resampler.
    write_wav(&wav, 44_100, 3.0);
    let (audio, events) = launch(SettingsStore::memory());

    let info = audio.load_track(deck_a(), &wav).expect("load_track");
    assert_eq!(info.file_name, "tone.wav");
    let loaded = wait_for(&events, |e| match e {
        CoreEvent::TrackLoaded(t) => Some(t.clone()),
        _ => None,
    });
    assert_eq!(loaded.deck, deck_a());
    let secs = loaded.frames.0 as f64 / f64::from(RATE);
    assert!((secs - 3.0).abs() < 0.01, "decoded {secs} s");
    assert!((loaded.track.duration_secs - 3.0).abs() < 0.01);

    let idle = wait_telemetry(&events, |t| t.decks[0].loaded);
    assert!(!idle.decks[0].playing);
    assert_eq!(idle.decks[0].position.0, 0);

    audio.set_playing(deck_a(), true).expect("play");
    let moving = wait_telemetry(&events, |t| {
        t.decks[0].playing && t.decks[0].position.0 > 4_800
    });
    assert!(moving.decks[0].position.0 < moving.decks[0].duration.0);

    audio.toggle_playing(deck_a()).expect("toggle");
    let stopped = wait_telemetry(&events, |t| !t.decks[0].playing);
    // Stays put once paused.
    std::thread::sleep(Duration::from_millis(100));
    let later = wait_telemetry(&events, |t| t.sequence > stopped.sequence + 2);
    assert_eq!(later.decks[0].position, stopped.decks[0].position);

    audio.toggle_playing(deck_a()).expect("toggle again");
    wait_telemetry(&events, |t| t.decks[0].playing);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn seek_moves_the_playhead() {
    let dir = tmp_dir("seek");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 4.0);
    let (audio, events) = launch(SettingsStore::memory());
    load(&audio, &events, &wav);

    audio.seek(deck_a(), Seconds(2.0)).expect("seek");
    let t = wait_telemetry(&events, |t| t.decks[0].position.0 >= 2 * u64::from(RATE));
    assert_eq!(t.decks[0].position.0, 2 * u64::from(RATE));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cue_returns_to_the_cue_point_and_previews_while_held() {
    let dir = tmp_dir("cue");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 4.0);
    let (audio, events) = launch(SettingsStore::memory());
    load(&audio, &events, &wav);

    // Playing, then CUE: back to the cue point (the start) and paused.
    audio.set_playing(deck_a(), true).expect("play");
    wait_telemetry(&events, |t| {
        t.decks[0].playing && t.decks[0].position.0 > 4_800
    });
    audio.cue(deck_a(), true).expect("cue down");
    audio.cue(deck_a(), false).expect("cue up");
    let back = wait_telemetry(&events, |t| !t.decks[0].playing);
    assert_eq!(back.decks[0].position.0, 0);

    // Paused on the cue point: holding CUE previews, release returns.
    audio.cue(deck_a(), true).expect("cue down");
    let preview = wait_telemetry(&events, |t| {
        t.decks[0].playing && t.decks[0].position.0 > 4_800
    });
    assert!(preview.decks[0].playing);
    audio.cue(deck_a(), false).expect("cue up");
    let released = wait_telemetry(&events, |t| !t.decks[0].playing);
    assert_eq!(released.decks[0].position.0, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_newer_load_cancels_the_older_one() {
    let dir = tmp_dir("cancel");
    let first = dir.join("first.wav");
    let second = dir.join("second.wav");
    write_wav(&first, 44_100, 20.0);
    write_wav(&second, RATE, 1.0);
    let (audio, events) = launch(SettingsStore::memory());

    audio.load_track(deck_a(), &first).expect("first");
    audio.load_track(deck_a(), &second).expect("second");
    let loaded = wait_for(&events, |e| match e {
        CoreEvent::TrackLoaded(t) => Some(t.clone()),
        _ => None,
    });
    assert_eq!(loaded.track.file_name, "second.wav");
    // No second TrackLoaded for the cancelled one.
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, CoreEvent::TrackLoaded(_)),
            "stale load delivered"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn undecodable_file_reports_a_failure_event() {
    let dir = tmp_dir("bad");
    let bad = dir.join("broken.wav");
    std::fs::write(&bad, b"this is not audio at all").expect("write");
    let (audio, events) = launch(SettingsStore::memory());

    // Tags are best effort; the failure comes from the decode.
    let _ = audio.load_track(deck_a(), &bad);
    let failed = wait_for(&events, |e| match e {
        CoreEvent::TrackLoadFailed(f) => Some(f.clone()),
        _ => None,
    });
    assert_eq!(failed.deck, deck_a());
    assert!(failed.path.ends_with("broken.wav"));
    assert!(!failed.message.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_file_is_an_immediate_error() {
    let (audio, _events) = launch(SettingsStore::memory());
    let err = audio
        .load_track(deck_a(), Path::new("/nonexistent/opendeck/nope.wav"))
        .unwrap_err();
    assert!(matches!(err, CoreError::Media(_)), "{err:?}");
}

#[test]
fn tracks_over_the_limit_are_refused() {
    let dir = tmp_dir("long");
    let wav = dir.join("long.wav");
    write_wav(&wav, RATE, 3.0);
    let core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        max_track_secs: 1.0,
        ..LaunchOptions::default()
    })
    .expect("launch");
    let err = core
        .audio()
        .expect("audio")
        .load_track(deck_a(), &wav)
        .unwrap_err();
    assert!(matches!(err, CoreError::TooLong { .. }), "{err:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settings_restart_the_engine_and_persist() {
    let dir = tmp_dir("settings");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 2.0);
    let (audio, events) = launch(SettingsStore::in_dir(&dir));
    load(&audio, &events, &wav);
    assert!(audio.status().expect("status").running);

    let settings = AudioSettings {
        buffer_frames: Some(256),
        headphones: Some(ChannelPair { left: 4, right: 5 }),
        ..AudioSettings::default()
    };
    let status = audio.set_audio_settings(settings.clone()).expect("apply");
    assert!(status.running);
    assert_eq!(status.headphones, Some(ChannelPair { left: 4, right: 5 }));
    assert_eq!(audio.audio_settings().expect("settings"), settings);
    // The loaded track is reloaded into the restarted engine.
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    wait_telemetry(&events, |t| t.decks[0].loaded);

    // A fresh launch on the same directory reads the stored settings.
    let (again, _events) = launch(SettingsStore::in_dir(&dir));
    assert_eq!(again.audio_settings().expect("settings"), settings);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tempo_fader_changes_the_playback_rate() {
    let dir = tmp_dir("tempo");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 4.0);
    let (audio, events) = launch(SettingsStore::memory());
    load(&audio, &events, &wav);

    audio
        .set_tempo_range(deck_a(), opendeck_contracts::tempo::TempoRange::Percent10)
        .expect("range");
    audio.set_tempo_fader(deck_a(), 1.0).expect("fader");
    audio.set_playing(deck_a(), true).expect("play");
    let t = wait_telemetry(&events, |t| {
        t.decks[0].playing && (t.decks[0].rate - 1.1).abs() < 0.01
    });
    assert!((t.decks[0].rate - 1.1).abs() < 0.01);
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- milestone 2: two decks, mixer, sync, cues -----

fn deck_b() -> DeckId {
    DeckId::new(1).expect("deck B")
}

fn load_into(audio: &AudioController, events: &Receiver<CoreEvent>, deck: DeckId, path: &Path) {
    audio.load_track(deck, path).expect("load_track");
    wait_for(events, |e| match e {
        CoreEvent::TrackLoaded(t) if t.deck == deck => Some(()),
        _ => None,
    });
}

#[test]
fn two_decks_play_independently() {
    let dir = tmp_dir("two");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 4.0);
    let (audio, events) = launch(SettingsStore::memory());
    load_into(&audio, &events, deck_a(), &wav);
    load_into(&audio, &events, deck_b(), &wav);

    audio.set_playing(deck_b(), true).expect("play B");
    let t = wait_telemetry(&events, |t| {
        t.decks[1].playing && t.decks[1].position.0 > 4_800
    });
    assert!(t.decks[1].loaded && t.decks[0].loaded);
    assert!(!t.decks[0].playing);
    assert_eq!(t.decks[0].position.0, 0);

    audio.seek(deck_b(), Seconds(2.0)).expect("seek B");
    wait_telemetry(&events, |t| t.decks[1].position.0 >= 2 * u64::from(RATE));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mixer_commands_are_forwarded_and_survive_an_engine_restart() {
    let dir = tmp_dir("mixer");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 3.0);
    let (audio, events) = launch(SettingsStore::memory());
    load_into(&audio, &events, deck_b(), &wav);

    audio.set_trim(deck_b(), Gain(1.5)).expect("trim");
    audio.set_eq(deck_b(), EqBand::Low, Gain(0.0)).expect("eq");
    audio.set_filter(deck_b(), -0.4).expect("filter");
    audio.set_channel_fader(deck_b(), 0.25).expect("fader");
    audio.set_pfl(deck_b(), true).expect("pfl");
    audio
        .set_crossfader_assign(deck_b(), CrossfaderAssign::B)
        .expect("assign");
    audio.set_crossfader(0.5).expect("crossfader");
    audio
        .set_crossfader_curve(CrossfaderCurve::Sharp)
        .expect("curve");
    audio.set_master_level(Gain(0.8)).expect("master");
    audio.set_headphone_level(Gain(0.6)).expect("hp level");
    audio.set_headphone_mix(0.75).expect("hp mix");
    audio.set_split_cue(true).expect("split");

    let check = |m: opendeck_contracts::mixer::MixerState| {
        let b = m.channels[1];
        assert_eq!(b.trim, Gain(1.5));
        assert_eq!(b.eq[0], Gain(0.0));
        assert_eq!(b.eq[1], Gain::UNITY);
        assert_eq!(b.filter, -0.4);
        assert_eq!(b.fader, 0.25);
        assert!(b.pfl);
        assert_eq!(b.crossfader_assign, CrossfaderAssign::B);
        assert_eq!(m.channels[0].fader, 1.0, "deck A untouched");
        assert_eq!(m.crossfader, 0.5);
        assert_eq!(m.crossfader_curve, CrossfaderCurve::Sharp);
        assert_eq!(m.master_level, Gain(0.8));
        assert_eq!(m.headphone_level, Gain(0.6));
        assert_eq!(m.headphone_mix, 0.75);
        assert!(m.split_cue);
    };
    check(audio.mixer_state().expect("mixer"));

    // The engine keeps accepting commands afterwards, and a restart replays them.
    audio.set_playing(deck_b(), true).expect("play");
    wait_telemetry(&events, |t| {
        t.decks[1].playing && t.decks[1].position.0 > 4_800
    });
    audio
        .set_audio_settings(AudioSettings {
            buffer_frames: Some(512),
            ..AudioSettings::default()
        })
        .expect("restart");
    check(audio.mixer_state().expect("mixer after restart"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_arguments_are_rejected() {
    let (audio, _events) = launch(SettingsStore::memory());
    assert!(matches!(
        audio.set_trim(deck_a(), Gain(f32::NAN)),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        audio.set_master_level(Gain(-1.0)),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        audio.auto_loop(deck_a(), Beats(0.0)),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        audio.beat_jump(deck_a(), Beats(0.0)),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        audio.resize_loop(deck_a(), -2.0),
        Err(CoreError::Invalid(_))
    ));
    assert!(matches!(
        audio.set_crossfader(f32::INFINITY),
        Err(CoreError::Invalid(_))
    ));
    // A deck without a track cannot store a cue.
    let index = HotCueIndex::new(0).expect("index");
    assert!(matches!(
        audio.set_hot_cue(deck_a(), index),
        Err(CoreError::Invalid(_))
    ));
}

#[test]
fn deck_controls_are_reported_by_deck_state() {
    let (audio, _events) = launch(SettingsStore::memory());
    audio.set_sync(deck_b(), SyncMode::Beat).expect("sync");
    audio.set_quantize(deck_b(), false).expect("quantize");
    audio.set_slip(deck_b(), true).expect("slip");
    audio.set_leader(Some(deck_b())).expect("leader");
    audio.set_vinyl_mode(deck_b(), false).expect("vinyl");
    audio.set_key_lock(deck_b(), true).expect("key lock");
    // Same-thread ordering: the query is answered after the commands above.
    let b = audio.deck_state(deck_b()).expect("deck B");
    assert_eq!(b.sync, SyncMode::Beat);
    assert!(!b.quantize && b.slip && b.is_leader && !b.vinyl_mode && b.key_lock);
    let a = audio.deck_state(deck_a()).expect("deck A");
    assert_eq!(a.sync, SyncMode::Off);
    assert!(a.quantize && !a.slip && !a.is_leader && a.vinyl_mode);
    // Loops, jumps and jog are forwarded without error.
    audio.loop_in(deck_a()).expect("loop in");
    audio.loop_out(deck_a()).expect("loop out");
    audio.loop_toggle(deck_a()).expect("loop toggle");
    audio.auto_loop(deck_a(), Beats(4.0)).expect("auto loop");
    audio
        .loop_roll(deck_a(), Beats(0.5), true)
        .expect("roll on");
    audio
        .loop_roll(deck_a(), Beats(0.5), false)
        .expect("roll off");
    audio.resize_loop(deck_a(), 0.5).expect("halve");
    audio.beat_jump(deck_a(), Beats(-8.0)).expect("jump");
    audio.jog_touch(deck_a(), true).expect("touch");
    audio.jog_turn(deck_a(), 0.01).expect("turn");
    audio.jog_search(deck_a(), Beats(4.0)).expect("search");
}

#[test]
fn hot_cues_are_kept_in_memory_and_cleared_on_eject() {
    let dir = tmp_dir("hotcue");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 4.0);
    let (audio, events) = launch(SettingsStore::memory());
    load_into(&audio, &events, deck_b(), &wav);

    audio.seek(deck_b(), Seconds(1.5)).expect("seek");
    wait_telemetry(&events, |t| {
        t.decks[1].position.0 == 3 * u64::from(RATE) / 2
    });
    let index = HotCueIndex::new(2).expect("index");
    let cue = audio.set_hot_cue(deck_b(), index).expect("set");
    assert!((cue.start.0 - 1.5).abs() < 1e-6);
    assert_eq!(cue.kind, CueKind::Hot { index });
    let changed = wait_for(&events, |e| match e {
        CoreEvent::HotCues(h) if h.deck == deck_b() => Some(h.clone()),
        _ => None,
    });
    assert_eq!(changed.slots.len(), 8);
    assert!(changed.slots[2].is_some());
    assert_eq!(changed.slots.iter().flatten().count(), 1);
    assert_eq!(audio.hot_cues(deck_b()).expect("cues"), changed);

    audio.delete_hot_cue(deck_b(), index).expect("delete");
    let cleared = wait_for(&events, |e| match e {
        CoreEvent::HotCues(h) if h.deck == deck_b() => Some(h.clone()),
        _ => None,
    });
    assert_eq!(cleared.slots.iter().flatten().count(), 0);

    audio.set_hot_cue(deck_b(), index).expect("set again");
    audio.eject(deck_b()).expect("eject");
    let after = audio.hot_cues(deck_b()).expect("cues");
    assert_eq!(after.slots.iter().flatten().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn memory_cues_are_recalled_in_order() {
    let dir = tmp_dir("memcue");
    let wav = dir.join("tone.wav");
    write_wav(&wav, RATE, 6.0);
    let (audio, events) = launch(SettingsStore::memory());
    load(&audio, &events, &wav);

    for secs in [3.0, 1.0] {
        audio.seek(deck_a(), Seconds(secs)).expect("seek");
        let frames = (secs * f64::from(RATE)) as u64;
        wait_telemetry(&events, |t| t.decks[0].position.0 == frames);
        audio.add_memory_cue(deck_a()).expect("memory cue");
    }
    audio.seek(deck_a(), Seconds(0.0)).expect("seek start");
    wait_telemetry(&events, |t| t.decks[0].position.0 == 0);

    audio.call_memory_cue(deck_a(), 1).expect("next");
    wait_telemetry(&events, |t| t.decks[0].position.0 == u64::from(RATE));
    audio.call_memory_cue(deck_a(), 1).expect("next");
    wait_telemetry(&events, |t| t.decks[0].position.0 == 3 * u64::from(RATE));
    audio.call_memory_cue(deck_a(), -1).expect("previous");
    wait_telemetry(&events, |t| t.decks[0].position.0 == u64::from(RATE));
    let _ = std::fs::remove_dir_all(&dir);
}
