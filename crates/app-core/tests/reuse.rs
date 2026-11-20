//! Stored analysis is reused across a restart: a library file is analysed in
//! one "process", the app quits, and a second one opens the same database
//! with an analyzer that fails the test if it is ever asked to work. Loading
//! the track onto a deck must give the stored BPM, key, grid and peaks, the
//! collection must still show it as analysed, and importing the same file
//! again must not touch the analysis.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AnalysisStatusDto, AppCore, Backend, CoreEvent, ImportOptionsDto, LaunchOptions, LibraryEvent,
    LibraryService, SettingsStore, SortColumnDto, TrackQueryDto,
};
use opendeck_contracts::DeckId;
use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::jobs::JobState;
use opendeck_library::import::ImportConfig;

const RATE: u32 = 44_100;
const WAIT: Duration = Duration::from_secs(60);
const BPM: f64 = 120.0;

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-reuse-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// 16-bit stereo WAV: a kick on every beat of a 4/4 bar at `BPM`, over a quiet noise bed.
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

/// An analyzer that counts its calls and fails them: the second "process"
/// must not need it.
struct Tripwire(Arc<AtomicUsize>);

impl Analyzer for Tripwire {
    fn id(&self) -> &str {
        "tripwire"
    }
    fn version(&self) -> u32 {
        1
    }
    fn features(&self) -> AnalysisFeatures {
        AnalysisFeatures::ALL
    }
    fn analyze(
        &self,
        _audio: AudioView<'_>,
        _out: &mut AnalysisResult,
        _cancel: &CancelFlag,
        _progress: &mut dyn FnMut(f32),
    ) -> Result<(), AnalysisError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(AnalysisError::InvalidInput("must not run".into()))
    }
}

fn wait_job(events: &Receiver<LibraryEvent>) {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(LibraryEvent::ImportProgress(p)) if p.state == JobState::Completed => return,
            Ok(LibraryEvent::ImportProgress(p)) if p.state == JobState::Failed => {
                panic!("job failed")
            }
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for the import job"),
        }
    }
}

fn all_rows(service: &LibraryService) -> Vec<opendeck_app_core::TrackRowDto> {
    service
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
            limit: 50,
        })
        .expect("query")
        .rows
}

#[test]
fn a_restarted_app_loads_an_analysed_track_from_the_stored_analysis() {
    let dir = tmp_dir("restart");
    let wav = dir.join("click.wav");
    write_click_track(&wav, 16.0);
    let db = dir.join(opendeck_app_core::DATABASE_FILE_NAME);

    // First run: real analyzers, import, quit.
    let (id, stored, peaks): (TrackId, _, Vec<u8>) = {
        let service = LibraryService::open(
            &db,
            LibraryService::default_analyzers(),
            ImportConfig::default(),
        )
        .expect("library");
        let events = service.subscribe();
        service
            .import_files(std::slice::from_ref(&wav), ImportOptionsDto::default())
            .expect("import");
        wait_job(&events);
        let row = all_rows(&service).pop().expect("imported row");
        assert_eq!(row.analysis_status, AnalysisStatusDto::Done);
        let stored = service
            .get_analysis(row.id)
            .expect("analysis")
            .expect("stored");
        assert!((stored.bpm.expect("bpm") - BPM).abs() < 0.5);
        let peaks = service
            .peaks(row.id, 3)
            .expect("peaks")
            .expect("overview peaks");
        (row.id, stored, peaks)
    };

    // Second run: the same file on disk, an analyzer that must never run.
    let calls = Arc::new(AtomicUsize::new(0));
    let analyzers: Vec<Arc<dyn Analyzer>> = vec![Arc::new(Tripwire(Arc::clone(&calls)))];
    let service = LibraryService::open(&db, analyzers, ImportConfig::default()).expect("reopen");
    let events = service.subscribe();
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

    // The collection still shows the analysis state.
    let rows = all_rows(&service);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    assert_eq!(rows[0].analysis_status, AnalysisStatusDto::Done);
    assert!(rows[0].bpm.is_some_and(|b| (b - BPM).abs() < 0.5));
    assert!(rows[0].key.is_some());

    // Loading onto a deck uses the stored values and peaks.
    let deck = DeckId::new(0).expect("deck A");
    let info = service.load_track(deck, id).expect("load by id");
    assert_eq!(info.bpm, stored.bpm);
    assert_eq!(info.key, stored.key.map(|k| k.camelot()));
    let deadline = Instant::now() + WAIT;
    let mut loaded = false;
    let mut gridded = false;
    while !(loaded && gridded) {
        let left = deadline.saturating_duration_since(Instant::now());
        match audio_events.recv_timeout(left) {
            Ok(CoreEvent::TrackLoaded(_)) => loaded = true,
            Ok(CoreEvent::TrackLoadFailed(f)) => panic!("load failed: {}", f.message),
            // The engine reports a BPM only once it holds the stored grid.
            Ok(CoreEvent::Telemetry(t)) => {
                gridded |= t.decks[0].bpm.is_some_and(|b| (b.0 - BPM).abs() < 0.5);
            }
            Ok(_) => {}
            Err(_) => panic!("the deck never got the stored grid"),
        }
    }
    assert_eq!(
        service.peaks(id, 3).expect("peaks").as_deref(),
        Some(peaks.as_slice()),
        "the stored overview peaks are served as they were written"
    );

    // Importing the same file again leaves the analysis alone.
    service
        .import_files(std::slice::from_ref(&wav), ImportOptionsDto::default())
        .expect("re-import");
    wait_job(&events);
    let again = service
        .get_analysis(id)
        .expect("analysis")
        .expect("still stored");
    assert_eq!(again, stored);
    assert_eq!(all_rows(&service).len(), 1, "no duplicate row");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "nothing was analysed again after the restart"
    );
    assert_eq!(
        all_rows(&service)[0].analysis_status,
        AnalysisStatusDto::Done
    );
    let _ = std::fs::remove_dir_all(&dir);
}
