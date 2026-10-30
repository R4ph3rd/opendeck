//! The MIDI service thread against the mock MIDI backend and a headless
//! `AppCore`: hot-plug, play and LED round trip, monitor, learn, persisted
//! overrides, export and import. No hardware, no audio device.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, Backend, CoreEvent, LaunchOptions, LibraryService, MidiBindingDto, MidiEvent,
    MidiMonitorLabelDto, MidiService, MidiServiceOptions, SettingsStore,
};
use opendeck_contracts::control::ControlAction;
use opendeck_library::import::ImportConfig;
use opendeck_midi::backend::mock::MockHandle;
use opendeck_midi::flx4_mapping;

const PORT: &str = "DDJ-FLX4 MIDI 1";
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-midisvc-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_wav(path: &std::path::Path) {
    let rate = 48_000u32;
    let frames = rate as usize * 3;
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
        let s = ((i as f64 * 0.03).sin() * 8000.0) as i16;
        bytes.extend(s.to_le_bytes());
        bytes.extend(s.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

struct Rig {
    core: AppCore,
    library: LibraryService,
    mock: MockHandle,
    service: MidiService,
    midi: Receiver<MidiEvent>,
    audio_events: Receiver<CoreEvent>,
}

fn options(core: &AppCore, library: &LibraryService, mock: &MockHandle) -> MidiServiceOptions {
    let backend = mock.clone();
    MidiServiceOptions {
        mapping: flx4_mapping().expect("mapping"),
        audio: core.audio().expect("audio"),
        library: Some(library.clone()),
        pad_modes: core.pad_modes(),
        store: Some(library.library()),
        backend: Some(Arc::new(move || {
            let backend = backend.clone();
            Box::new(move || Ok(Box::new(backend.backend()) as _))
        })),
        rescan_interval: Some(Duration::from_millis(10)),
    }
}

impl Rig {
    fn new() -> Self {
        let library =
            LibraryService::open_in_memory(Vec::new(), ImportConfig::default()).expect("library");
        let core = AppCore::launch_with_library(
            LaunchOptions {
                backend: Backend::Offline {
                    sample_rate: 48_000,
                },
                store: SettingsStore::memory(),
                ..LaunchOptions::default()
            },
            library.clone(),
        )
        .expect("launch");
        let mock = MockHandle::default();
        let service = MidiService::start(options(&core, &library, &mock)).expect("service");
        let midi = service.subscribe().expect("subscribe");
        let audio_events = core.audio().expect("audio").subscribe().expect("events");
        Self {
            core,
            library,
            mock,
            service,
            midi,
            audio_events,
        }
    }

    fn wait_event<T>(&self, what: &str, mut pick: impl FnMut(&MidiEvent) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.midi.recv_timeout(left) {
                Ok(event) => {
                    if let Some(found) = pick(&event) {
                        return found;
                    }
                }
                Err(_) => panic!("timed out waiting for {what}"),
            }
        }
    }

    fn wait_until(&self, what: &str, mut cond: impl FnMut() -> bool) {
        let deadline = Instant::now() + WAIT;
        while Instant::now() < deadline {
            if cond() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("timed out waiting for {what}");
    }

    fn playing(&self) -> bool {
        let mut playing = false;
        while let Ok(event) = self.audio_events.try_recv() {
            if let CoreEvent::Telemetry(t) = event {
                playing = t.decks[0].playing;
            }
        }
        playing
    }
}

#[test]
fn plugging_the_controller_connects_plays_and_lights_leds() {
    let rig = Rig::new();
    let dir = tmp_dir("play");
    let wav = dir.join("tone.wav");
    write_wav(&wav);
    rig.core
        .load_track_path(opendeck_contracts::DeckId::new(0).expect("deck"), &wav)
        .expect("load");

    let status = rig.service.status().expect("status");
    assert!(status.enabled && !status.connected);
    assert!(status.verify_count > 0);

    rig.mock.plug("Other Device");
    rig.mock.plug(PORT);
    rig.wait_event("connected status", |e| match e {
        MidiEvent::Status(s) if s.connected => Some(()),
        _ => None,
    });
    assert_eq!(
        rig.service.status().expect("status").input.as_deref(),
        Some(PORT)
    );

    // The LEDs for the current state arrive on connect (a loaded paused deck blinks).
    rig.wait_until("PLAY LED", || {
        rig.mock
            .sent()
            .iter()
            .any(|(port, bytes)| port == PORT && bytes == &[144, 11, 127])
    });

    rig.service.set_monitor(true).expect("monitor");
    assert!(rig.mock.push(PORT, 1_000, &[144, 11, 127]));
    assert!(rig.mock.push(PORT, 2_000, &[144, 11, 0]));
    rig.wait_until("deck plays", || rig.playing());

    let entry = rig.wait_event("monitor entry", |e| match e {
        MidiEvent::Monitor(batch) => batch
            .iter()
            .find(|m| matches!(m.label, MidiMonitorLabelDto::Control { .. }))
            .cloned(),
        _ => None,
    });
    match entry.label {
        MidiMonitorLabelDto::Control { id, action, .. } => {
            assert_eq!(id, "deck1.play");
            assert!(matches!(action, Some(ControlAction::PlayPause { .. })));
        }
        other => panic!("unexpected label {other:?}"),
    }

    // Unplugging shows as disconnected, the worker keeps looking.
    rig.mock.unplug(PORT);
    rig.wait_event("disconnected status", |e| match e {
        MidiEvent::Status(s) if !s.connected && s.enabled => Some(()),
        _ => None,
    });
    let _ = rig.library;
}

#[test]
fn learn_stores_an_override_that_survives_a_restart_and_exports() {
    let rig = Rig::new();
    rig.mock.plug(PORT);
    rig.wait_event("connected", |e| match e {
        MidiEvent::Status(s) if s.connected => Some(()),
        _ => None,
    });

    // Learn: PLAY of deck 1 moves to note 0x14 on channel 1.
    rig.service.learn_start("deck1.play".into()).expect("learn");
    // Arming is queued to the worker, so repeat the gesture until it is captured.
    let deadline = Instant::now() + WAIT;
    let result = loop {
        rig.mock.push(PORT, 1_000, &[144, 20, 127]);
        match rig.midi.recv_timeout(Duration::from_millis(100)) {
            Ok(MidiEvent::Learn(r)) => break r,
            _ => assert!(
                Instant::now() < deadline,
                "timed out waiting for the learn result"
            ),
        }
    };
    assert_eq!(result.error, None, "{result:?}");
    assert_eq!(
        result.binding,
        Some(MidiBindingDto::Note {
            channel: 1,
            number: 20
        })
    );
    let rows = rig.service.mapping_rows().expect("rows");
    let row = rows.iter().find(|r| r.id == "deck1.play").expect("row");
    assert!(row.overridden);
    assert_eq!(
        row.binding,
        MidiBindingDto::Note {
            channel: 1,
            number: 20
        }
    );

    // The restarted worker listens to the new note.
    rig.wait_until("reconnected", || rig.mock.input_open(PORT));
    let dir = tmp_dir("learn");
    let wav = dir.join("tone.wav");
    write_wav(&wav);
    rig.core
        .load_track_path(opendeck_contracts::DeckId::new(0).expect("deck"), &wav)
        .expect("load");
    rig.wait_until("track loaded", || {
        rig.core
            .audio()
            .and_then(|a| a.deck_state(opendeck_contracts::DeckId::new(0).expect("deck")))
            .is_ok()
    });
    std::thread::sleep(Duration::from_millis(300));
    rig.mock.push(PORT, 3_000, &[144, 20, 127]);
    rig.wait_until("new binding plays", || rig.playing());

    // Export, reset, import restores the override.
    let file = dir.join("mapping.json");
    rig.service
        .export_mapping(file.to_string_lossy().into_owned())
        .expect("export");
    let rows = rig.service.reset_overrides().expect("reset");
    assert!(rows.iter().all(|r| !r.overridden));
    let rows = rig
        .service
        .import_mapping(file.to_string_lossy().into_owned())
        .expect("import");
    assert!(rows.iter().any(|r| r.id == "deck1.play" && r.overridden));

    // A new service on the same library reads the stored override.
    let second = MidiService::start(options(&rig.core, &rig.library, &rig.mock)).expect("second");
    let rows = second.mapping_rows().expect("rows");
    assert!(rows.iter().any(|r| r.id == "deck1.play" && r.overridden));

    // An input another control already uses is refused and nothing changes.
    let err = rig
        .service
        .set_override(
            "deck1.cue".into(),
            MidiBindingDto::Note {
                channel: 1,
                number: 20,
            },
        )
        .expect_err("clash");
    assert!(err.to_string().contains("deck1"), "{err}");
    let rows = rig.service.mapping_rows().expect("rows");
    assert!(rows.iter().any(|r| r.id == "deck1.play" && r.overridden));
    assert!(rows.iter().all(|r| r.id != "deck1.cue" || !r.overridden));

    // Resetting one control restores the shipped binding.
    let rows = rig
        .service
        .clear_override("deck1.play".into())
        .expect("clear");
    assert!(rows.iter().all(|r| !r.overridden));
}

#[test]
fn manual_device_selection_and_disconnect() {
    let rig = Rig::new();
    rig.mock.plug("Generic MIDI Controller");
    // The default patterns do not match, the manual choice does.
    std::thread::sleep(Duration::from_millis(100));
    assert!(!rig.service.status().expect("status").connected);
    let status = rig
        .service
        .connect(Some("Generic MIDI Controller".into()))
        .expect("connect");
    assert_eq!(status.device.as_deref(), Some("Generic MIDI Controller"));
    rig.wait_until("manual device open", || {
        rig.mock.input_open("Generic MIDI Controller")
    });

    let status = rig.service.disconnect().expect("disconnect");
    assert!(!status.enabled && !status.connected);
    rig.wait_until("ports closed", || {
        !rig.mock.input_open("Generic MIDI Controller")
    });
    let status = rig.service.set_auto_connect(false).expect("auto");
    assert!(!status.auto_connect);
    assert!(rig.service.settings().expect("settings").device.is_some());
}
