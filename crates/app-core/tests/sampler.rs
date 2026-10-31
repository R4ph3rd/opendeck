//! The sampler's slot assignment survives a restart (library settings), and
//! a pad press in SAMPLER mode plays the slot through the engine.

use std::path::Path;
use std::time::{Duration, Instant};

use opendeck_app_core::{
    AppCore, Backend, CoreEvent, LaunchOptions, LibraryService, SamplerLayout, SamplerPlayMode,
    SamplerSlotPatch, SettingsStore,
};
use opendeck_contracts::DeckId;
use opendeck_contracts::pad::PadMode;
use opendeck_library::import::ImportConfig;

const RATE: u32 = 48_000;

fn write_wav(path: &Path) {
    write_wav_secs(path, 0.25);
}

/// A 16-bit stereo sine at the engine rate, `secs` long.
fn write_wav_secs(path: &Path, secs: f64) {
    let frames = (f64::from(RATE) * secs) as usize;
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
        let q = ((i as f64 * 0.05).sin() * 12_000.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn launch(db: &Path) -> AppCore {
    let library = LibraryService::open(db, Vec::new(), ImportConfig::default()).expect("library");
    AppCore::launch_with_library(
        LaunchOptions {
            backend: Backend::Offline { sample_rate: RATE },
            store: SettingsStore::memory(),
            ..LaunchOptions::default()
        },
        library,
    )
    .expect("launch")
}

fn wait(what: &str, cond: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cond() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn slot_assignment_and_settings_persist() {
    let dir = std::env::temp_dir().join(format!("opendeck-sampler-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let wav = dir.join("clap.wav");
    write_wav(&wav);
    let db = dir.join("lib.sqlite");

    {
        let core = launch(&db);
        let sampler = core.sampler();
        sampler.load_path(3, &wav, None).expect("load");
        wait("loaded", || sampler.state().slots[3].loaded);
        sampler
            .patch(
                3,
                SamplerSlotPatch {
                    name: Some("Clap".into()),
                    gain: Some(0.5),
                    play_mode: Some(SamplerPlayMode::Gate),
                    sync: Some(true),
                    color: Some(5),
                },
            )
            .expect("patch");
        sampler.set_layout(SamplerLayout::Shared);
        sampler.set_level(0.25).expect("level");
        sampler.set_pfl(true).expect("pfl");
        let slot = &sampler.state().slots[3];
        assert!(slot.peaks.iter().any(|p| *p > 0.1));
        assert!((slot.duration_secs - 0.25).abs() < 0.01);
    }

    let core = launch(&db);
    let sampler = core.sampler();
    wait("reloaded", || sampler.state().slots[3].loaded);
    let state = sampler.state();
    let slot = &state.slots[3];
    assert_eq!(slot.name, "Clap");
    assert_eq!(slot.play_mode, SamplerPlayMode::Gate);
    assert!(slot.sync);
    assert_eq!(slot.color, 5);
    assert!((slot.gain - 0.5).abs() < 1e-6);
    assert_eq!(state.layout, SamplerLayout::Shared);
    assert!((state.level - 0.25).abs() < 1e-6);
    assert!(state.pfl);

    sampler.clear(3).expect("clear");
    assert!(!sampler.state().slots[3].loaded);
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Waits for a telemetry snapshot where slot `slot` is (or is not) playing.
fn wait_playing(events: &crossbeam_channel::Receiver<CoreEvent>, slot: usize, playing: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "timed out: slot {slot} playing={playing}");
        if let Ok(CoreEvent::Telemetry(t)) = events.recv_timeout(left)
            && t.sampler[slot].loaded
            && t.sampler[slot].playing == playing
        {
            return;
        }
    }
}

#[test]
fn sampler_pad_plays_slot_one_and_gate_stops_on_release() {
    let dir = std::env::temp_dir().join(format!("opendeck-sampler-pad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let wav = dir.join("tone.wav");
    write_wav_secs(&wav, 3.0);

    let mut core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        store: SettingsStore::memory(),
        ..LaunchOptions::default()
    })
    .expect("launch");
    let audio = core.audio().expect("audio");
    let events = audio.subscribe().expect("subscribe");
    let sampler = core.sampler();
    sampler.load_path(0, &wav, None).expect("load");
    wait("slot 1 loaded", || sampler.state().slots[0].loaded);
    sampler
        .patch(
            0,
            SamplerSlotPatch {
                play_mode: Some(SamplerPlayMode::Gate),
                ..SamplerSlotPatch::default()
            },
        )
        .expect("gate");

    let deck = DeckId::new(0).expect("deck");
    core.set_pad_mode(deck, PadMode::Sampler);
    let pads = core.pad_actions().expect("pads");

    let before = audio.render_offline(4_800).expect("render");
    assert!(
        peak(&before.master) < 1e-4,
        "master is silent before the press"
    );

    pads.press(deck, PadMode::Sampler, 0, false, true, None);
    let held = audio.render_offline(4_800).expect("render");
    assert!(
        peak(&held.master) > 0.1,
        "the sample reaches the master: peak {}",
        peak(&held.master)
    );
    wait_playing(&events, 0, true);

    pads.press(deck, PadMode::Sampler, 0, false, false, None);
    let released = audio.render_offline(4_800).expect("render");
    // The gate fades out over 2 ms (96 frames); skip 4 ms.
    assert!(
        peak(&released.master[2 * 192..]) < 1e-4,
        "the gate stops on release: peak {}",
        peak(&released.master[2 * 192..])
    );
    wait_playing(&events, 0, false);

    // An engine restart at the same rate keeps the sample and its mode.
    audio
        .set_audio_settings(audio.audio_settings().expect("settings"))
        .expect("restart");
    pads.press(deck, PadMode::Sampler, 0, false, true, None);
    let again = audio.render_offline(4_800).expect("render");
    assert!(peak(&again.master) > 0.1, "the slot survives a restart");
    pads.press(deck, PadMode::Sampler, 0, false, false, None);
    let after = audio.render_offline(4_800).expect("render");
    assert!(
        peak(&after.master[2 * 192..]) < 1e-4,
        "still a gate after a restart"
    );
}
