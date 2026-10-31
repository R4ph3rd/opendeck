//! The sampler's slot assignment survives a restart (library settings).

use std::path::Path;
use std::time::{Duration, Instant};

use opendeck_app_core::{
    AppCore, Backend, LaunchOptions, LibraryService, SamplerLayout, SamplerPlayMode,
    SamplerSlotPatch, SettingsStore,
};
use opendeck_library::import::ImportConfig;

const RATE: u32 = 48_000;

fn write_wav(path: &Path) {
    let frames = RATE as usize / 4;
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
