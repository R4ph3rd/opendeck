//! Library integration: import generated WAVs with the real analyzers into an
//! in-memory library, load by `TrackId`, persist cues, record the session set.
//! Runs headless on the offline audio backend.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreEvent, ImportOptionsDto, LaunchOptions, LibraryEvent,
    LibraryService, SettingsStore, SortColumnDto, TrackQueryDto,
};
use opendeck_contracts::cue::{CueKind, HotCueIndex};
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::jobs::JobState;
use opendeck_contracts::{DeckId, Seconds};
use opendeck_library::import::ImportConfig;

const RATE: u32 = 44_100;
const WAIT: Duration = Duration::from_secs(60);
const BPM: f64 = 120.0;

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-lib-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// 16-bit stereo WAV: a kick on every beat of a 4/4 bar at `BPM`, louder on
/// the downbeat, over a quiet noise bed.
fn write_click_track(path: &Path, secs: f64) {
    let frames = (f64::from(RATE) * secs) as usize;
    let beat = 60.0 / BPM;
    let mut noise = 0x2545_f491_u32;
    let mut bytes = Vec::new();
    let data_len = (frames * 4) as u32;
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
        let t = i as f64 / f64::from(RATE);
        let beat_index = (t / beat).floor();
        let since = t - beat_index * beat;
        let accent = if (beat_index as u64).is_multiple_of(4) {
            1.0
        } else {
            0.7
        };
        let kick = (std::f64::consts::TAU * (55.0 + 90.0 * (-since * 40.0).exp()) * since).sin()
            * (-since * 14.0).exp()
            * accent;
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let hiss = (f64::from(noise >> 16) / 32768.0 - 1.0) * 0.01;
        let q = ((kick * 0.8 + hiss) * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

struct Rig {
    core: AppCore,
    service: LibraryService,
    audio: AudioController,
    audio_events: Receiver<CoreEvent>,
    library_events: Receiver<LibraryEvent>,
}

fn rig() -> Rig {
    let service = LibraryService::open_in_memory(
        LibraryService::default_analyzers(),
        ImportConfig::default(),
    )
    .expect("library");
    let library_events = service.subscribe();
    let core = AppCore::launch_with_library(
        LaunchOptions {
            backend: Backend::Offline { sample_rate: RATE },
            store: SettingsStore::memory(),
            ..LaunchOptions::default()
        },
        service.clone(),
    )
    .expect("launch");
    let audio = core.audio().expect("audio");
    let audio_events = audio.subscribe().expect("subscribe");
    Rig {
        core,
        service,
        audio,
        audio_events,
        library_events,
    }
}

fn deck_a() -> DeckId {
    DeckId::new(0).expect("deck A")
}

fn wait_job(rig: &Rig) {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rig.library_events.recv_timeout(left) {
            Ok(LibraryEvent::ImportProgress(p)) if p.state == JobState::Completed => return,
            Ok(LibraryEvent::ImportProgress(p)) if p.state == JobState::Failed => {
                panic!("job failed")
            }
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for the import job"),
        }
    }
}

fn wait_loaded(rig: &Rig) {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rig.audio_events.recv_timeout(left) {
            Ok(CoreEvent::TrackLoaded(_)) => return,
            Ok(CoreEvent::TrackLoadFailed(f)) => panic!("load failed: {}", f.message),
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for track-loaded"),
        }
    }
}

fn wait_telemetry(rig: &Rig, ok: impl Fn(&opendeck_contracts::EngineTelemetry) -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rig.audio_events.recv_timeout(left) {
            Ok(CoreEvent::Telemetry(t)) if ok(&t) => return,
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for telemetry"),
        }
    }
}

fn import_one(rig: &Rig, wav: &Path) -> TrackId {
    rig.service
        .import_files(&[wav.to_path_buf()], ImportOptionsDto::default())
        .expect("import");
    wait_job(rig);
    let page = rig
        .service
        .query_tracks(TrackQueryDto {
            text: None,
            bpm_min: None,
            bpm_max: None,
            keys: vec![],
            genre: None,
            min_rating: None,
            status: None,
            sort: SortColumnDto::DateAdded,
            descending: true,
            offset: 0,
            limit: 10,
        })
        .expect("query");
    page.rows
        .iter()
        .find(|r| r.path.ends_with(wav.file_name().unwrap().to_str().unwrap()))
        .expect("imported row")
        .id
}

#[test]
fn imported_track_loads_by_id_with_its_grid() {
    let dir = tmp_dir("grid");
    let wav = dir.join("click.wav");
    write_click_track(&wav, 16.0);
    let rig = rig();
    let id = import_one(&rig, &wav);

    let detail = rig.service.get_track(id).expect("detail");
    assert_eq!(
        detail.analysis_status,
        opendeck_app_core::AnalysisStatusDto::Done
    );
    let analysis = rig
        .service
        .get_analysis(id)
        .expect("analysis")
        .expect("stored");
    let bpm = analysis.bpm.expect("bpm");
    assert!((bpm - BPM).abs() < 0.5, "analysed {bpm} BPM");
    assert!(analysis.beat_grid.is_some());
    assert!(analysis.key.is_some());

    let info = rig.service.load_track(deck_a(), id).expect("load by id");
    assert_eq!(info.id, Some(id));
    assert!((info.bpm.expect("info bpm") - BPM).abs() < 0.5);
    assert!(info.key.is_some() && info.key_musical.is_some());
    wait_loaded(&rig);
    // The engine only reports a BPM once it holds the grid.
    wait_telemetry(&rig, |t| {
        t.decks[0].bpm.is_some_and(|b| (b.0 - BPM).abs() < 0.5)
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cues_persist_and_come_back_on_reload() {
    let dir = tmp_dir("cues");
    let wav = dir.join("click.wav");
    write_click_track(&wav, 8.0);
    let rig = rig();
    let id = import_one(&rig, &wav);

    rig.service.load_track(deck_a(), id).expect("load");
    wait_loaded(&rig);
    rig.audio.seek(deck_a(), Seconds(2.0)).expect("seek");
    wait_telemetry(&rig, |t| t.decks[0].position.0 > i64::from(RATE) as u64);

    let index = HotCueIndex::new(2).expect("index");
    let cue = rig.audio.set_hot_cue(deck_a(), index).expect("set hot cue");
    assert!(cue.id.is_some(), "the stored cue carries its row id");
    let memory = rig.audio.add_memory_cue(deck_a()).expect("memory cue");
    assert!(memory.id.is_some());

    let stored = rig.service.list_cues(id).expect("cues");
    assert_eq!(stored.len(), 2);
    let hot = stored
        .iter()
        .find(|c| c.kind == CueKind::Hot { index })
        .expect("hot cue row");
    assert!((hot.start.0 - cue.start.0).abs() < 1e-9);
    assert!(stored.iter().any(|c| c.kind == CueKind::Memory));

    // Eject, load again: the cues come from the database.
    rig.core.eject(deck_a()).expect("eject");
    assert!(rig.audio.hot_cues(deck_a()).expect("cues").slots[2].is_none());
    rig.service.load_track(deck_a(), id).expect("reload");
    wait_loaded(&rig);
    let restored = rig.audio.hot_cues(deck_a()).expect("cues");
    let again = restored.slots[2].as_ref().expect("hot cue restored");
    assert!((again.start.0 - cue.start.0).abs() < 1e-9);
    assert_eq!(again.id, cue.id);

    // Deleting a hot cue deletes the row.
    rig.audio.delete_hot_cue(deck_a(), index).expect("delete");
    rig.audio
        .hot_cues(deck_a())
        .expect("sync with the control thread");
    let left = rig.service.list_cues(id).expect("cues");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].kind, CueKind::Memory);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loaded_tracks_join_the_session_set_and_count_as_played() {
    let dir = tmp_dir("set");
    let wav = dir.join("click.wav");
    write_click_track(&wav, 6.0);
    let rig = rig();
    let id = import_one(&rig, &wav);

    rig.service.load_track(deck_a(), id).expect("load");
    wait_loaded(&rig);
    // Loading it onto the other deck adds a second entry.
    let deck_b = DeckId::new(1).expect("deck B");
    rig.service.load_track(deck_b, id).expect("load B");
    wait_loaded(&rig);

    let sets = rig.service.list_saved_sets().expect("sets");
    assert_eq!(sets.len(), 1, "one set per session");
    assert!(sets[0].name.starts_with("Session "));
    let detail = rig.service.get_saved_set(sets[0].id).expect("set");
    let decks: Vec<_> = detail.entries.iter().map(|e| e.deck).collect();
    assert_eq!(decks, [Some(0), Some(1)]);
    assert!(detail.entries.iter().all(|e| e.track == Some(id)));
    assert_eq!(rig.service.get_track(id).expect("detail").play_count, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loading_a_file_outside_the_library_imports_it_first() {
    let dir = tmp_dir("path");
    let wav = dir.join("fresh.wav");
    write_click_track(&wav, 6.0);
    let rig = rig();

    let info = rig
        .service
        .load_track_path(deck_a(), &wav)
        .expect("load by path");
    let id = info.id.expect("the file became a library track");
    wait_loaded(&rig);
    assert_eq!(rig.service.get_track(id).expect("row").path, {
        opendeck_library::normalize_path_string(&wav)
    });

    // Loading the same file again reuses the row.
    let again = rig
        .service
        .load_track_path(deck_a(), &wav)
        .expect("second load");
    assert_eq!(again.id, Some(id));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn analysis_finishing_after_the_load_reaches_the_deck() {
    let dir = tmp_dir("late");
    let wav = dir.join("late.wav");
    write_click_track(&wav, 16.0);
    let rig = rig();

    // Loaded straight after the import row exists, usually before analysis ends.
    let info = rig.service.load_track_path(deck_a(), &wav).expect("load");
    let id = info.id.expect("track id");
    wait_loaded(&rig);
    // Whether or not the grid was ready at load time, the deck ends up with it.
    wait_telemetry(&rig, |t| {
        t.decks[0].bpm.is_some_and(|b| (b.0 - BPM).abs() < 0.5)
    });
    assert_eq!(
        rig.service.get_track(id).expect("row").analysis_status,
        opendeck_app_core::AnalysisStatusDto::Done
    );
    let _ = std::fs::remove_dir_all(&dir);
}
