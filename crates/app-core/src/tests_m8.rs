//! Control-thread hardening tests that need crate-private hooks: a simulated
//! device loss with automatic reopen, and frees on the loader thread.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_contracts::{DeckId, EngineTelemetry, Seconds};

use crate::control::Msg;
use crate::loader::FREED;
use crate::{AudioController, Backend, CoreEvent, LaunchOptions};

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn write_wav(path: &Path, secs: f32) {
    let frames = (RATE as f32 * secs) as usize;
    let data_len = (frames * 4) as u32;
    let mut bytes = Vec::new();
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
        let s = (std::f64::consts::TAU * 220.0 * i as f64 / f64::from(RATE)).sin() * 0.5;
        let q = (s * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn tmp_wav(name: &str, secs: f32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-m8-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("tone.wav");
    write_wav(&path, secs);
    path
}

fn launch() -> (AudioController, Receiver<CoreEvent>) {
    let audio = AudioController::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        ..LaunchOptions::default()
    })
    .expect("launch");
    let events = audio.subscribe().expect("subscribe");
    (audio, events)
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

fn deck_a() -> DeckId {
    DeckId::new(0).expect("deck A")
}

#[test]
fn a_lost_device_is_reopened_and_the_deck_resumes() {
    let wav = tmp_wav("loss", 30.0);
    let (audio, events) = launch();
    audio.load_track(deck_a(), &wav).expect("load");
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    audio.seek(deck_a(), Seconds(10.0)).expect("seek");
    audio.set_playing(deck_a(), true).expect("play");
    let before = wait_telemetry(&events, |t| {
        t.decks[0].playing && t.decks[0].position.0 > 10 * u64::from(RATE) + 4_800
    });

    audio.post(Msg::SimulateDeviceLoss).expect("post");
    let down = wait_for(&events, |e| match e {
        CoreEvent::AudioStatus(s) if !s.running => Some(s.clone()),
        _ => None,
    });
    assert!(
        down.reconnecting,
        "the core says it is waiting for the device"
    );
    assert!(down.error.is_some());

    // The first attempt is due after 250 ms and brings the engine back.
    let up = wait_for(&events, |e| match e {
        CoreEvent::AudioStatus(s) if s.running => Some(s.clone()),
        _ => None,
    });
    assert!(!up.reconnecting);
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    let after = wait_telemetry(&events, |t| t.decks[0].playing && t.decks[0].loaded);
    assert!(
        after.decks[0].position.0 >= before.decks[0].position.0,
        "resumed at {} after {}",
        after.decks[0].position.0,
        before.decks[0].position.0
    );
    assert!(audio.status().expect("status").running);
}

#[test]
fn replaced_track_buffers_are_dropped_on_the_loader_thread() {
    let first = tmp_wav("free-a", 5.0);
    let second = tmp_wav("free-b", 5.0);
    let (audio, events) = launch();
    audio.load_track(deck_a(), &first).expect("load");
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    let before = FREED.load(Ordering::Relaxed);
    // Loading a second track hands the first buffer back on the return ring.
    audio.load_track(deck_a(), &second).expect("load");
    wait_for(&events, |e| {
        matches!(e, CoreEvent::TrackLoaded(_)).then_some(())
    });
    let deadline = Instant::now() + WAIT;
    while FREED.load(Ordering::Relaxed) == before {
        assert!(Instant::now() < deadline, "no buffer reached the loader");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn test_tone_plays_through_the_engine_on_routed_outputs() {
    let (audio, _events) = launch();
    let amp = |bus: &[f32]| bus.iter().fold(0.0f32, |m, s| m.max(s.abs()));

    audio
        .play_test_tone(crate::TestToneOutput::Headphones)
        .expect("headphones are routed by default");
    let out = audio.render_offline(RATE as usize / 4).expect("render");
    assert!(amp(&out.headphones) > 0.15, "headphones carry the tone");
    assert!(amp(&out.master) < 1e-6, "master stays silent");

    let err = audio
        .play_test_tone(crate::TestToneOutput::Booth)
        .expect_err("no booth pair by default");
    assert!(err.to_string().contains("booth"), "{err}");
}
