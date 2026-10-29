//! Import queue tests: folder import, dedupe, cancel, pause, resume, re-analysis,
//! M3U and rekordbox XML, with generated WAV fixtures and a fake analyzer.
#![allow(missing_docs)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{FakeAnalyzer, tmp, wait_terminal, write_wav};
use crossbeam_channel::Receiver;
use opendeck_contracts::analysis::{AnalysisFeatures, Analyzer};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::CueKind;
use opendeck_contracts::ids::JobId;
use opendeck_contracts::jobs::{JobItemStatus, JobKind, JobProgress, JobState};
use opendeck_contracts::units::{Bpm, Seconds};
use opendeck_library::import::{ImportConfig, ImportManager, ImportOptions};
use opendeck_library::{
    AnalysisStatus, DATABASE_FILE_NAME, Library, TrackQuery, normalize_path_string,
};

const WAIT: Duration = Duration::from_secs(30);

fn config(workers: usize) -> ImportConfig {
    ImportConfig {
        workers,
        progress_interval: Duration::from_millis(20),
        ..ImportConfig::default()
    }
}

struct Rig {
    lib: Library,
    mgr: ImportManager,
    rx: Receiver<JobProgress>,
    analyzer: Arc<FakeAnalyzer>,
}

fn rig_with(lib: Library, delay: Duration, workers: usize) -> Rig {
    let analyzer = Arc::new(FakeAnalyzer::new(delay));
    let a: Arc<dyn Analyzer> = analyzer.clone();
    let (mgr, rx) = ImportManager::new(lib.clone(), vec![a], config(workers));
    Rig {
        lib,
        mgr,
        rx,
        analyzer,
    }
}

fn rig(delay: Duration, workers: usize) -> Rig {
    rig_with(Library::open_in_memory().unwrap(), delay, workers)
}

/// `n` distinct tones in `dir`, named `track-NN.wav`.
fn make_wavs(dir: &Path, n: usize) -> Vec<PathBuf> {
    (0..n)
        .map(|i| {
            let p = dir.join(format!("track-{i:02}.wav"));
            write_wav(&p, 22_050, 0.5, 200.0 + 37.0 * i as f32);
            p
        })
        .collect()
}

fn all_tracks(lib: &Library) -> Vec<opendeck_library::TrackRow> {
    lib.query_tracks(&TrackQuery {
        limit: u32::MAX,
        sort: opendeck_library::SortColumn::Title,
        ..TrackQuery::default()
    })
    .unwrap()
    .rows
}

fn last(events: &[JobProgress]) -> &JobProgress {
    events.last().expect("events")
}

#[test]
fn folder_import_runs_the_whole_pipeline() {
    let dir = tmp("folder");
    make_wavs(&dir.join("album one"), 3);
    make_wavs(&dir.join("deep/er/album two"), 2);
    // Same bytes as an existing file at a different path: a duplicate.
    std::fs::copy(
        dir.join("album one/track-00.wav"),
        dir.join("copy-of-00.wav"),
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "not audio").unwrap();
    std::fs::write(dir.join("album one/.hidden.wav"), "ignored").unwrap();

    let r = rig(Duration::ZERO, 3);
    let job = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    let events = wait_terminal(&r.rx, job, WAIT);
    let end = last(&events);
    assert_eq!(end.state, JobState::Completed);
    assert_eq!(end.kind, JobKind::ImportFolder);
    // album one (3) + album two (2) + the copy; track-00.wav exists in both albums with the same tone.
    assert_eq!(end.items_total, 6);
    assert_eq!(end.items_done + end.items_duplicate + end.items_failed, 6);
    assert!(end.items_failed == 0, "no failures: {end:?}");
    assert_eq!(
        end.items_duplicate, 3,
        "the copy plus two same-tone files across albums"
    );

    // Progress is monotonic and never reports more than the total.
    let mut prev = 0;
    for e in &events {
        let fin = e.items_done + e.items_duplicate + e.items_failed;
        assert!(fin >= prev && fin <= e.items_total);
        prev = fin;
    }

    let tracks = all_tracks(&r.lib);
    assert_eq!(tracks.len(), 3);
    for t in &tracks {
        assert_eq!(t.analysis_status, AnalysisStatus::Done);
        assert_eq!(t.bpm, Some(120.0));
        assert_eq!(t.key.as_deref(), Some("8A"));
        let stem = Path::new(&t.path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            t.title.as_deref(),
            Some(stem.as_str()),
            "title falls back to the file name"
        );
        let full = r.lib.get_track(t.id).unwrap().unwrap();
        assert!((full.tech.duration_secs.unwrap() - 0.5).abs() < 0.05);
        assert_eq!(full.tech.sample_rate, Some(22_050));
        assert_eq!(full.tech.codec.as_deref(), Some("wav"));
        assert!(full.content_hash.is_some() && full.file_size > 0);
        let a = r.lib.get_analysis(t.id).unwrap().unwrap();
        assert_eq!((a.analyzer.as_str(), a.sample_rate), ("fake", 44_100));
        assert_eq!(r.lib.peak_levels(t.id).unwrap().len(), 2);
        assert!(a.beat_grid.is_some());
    }
    assert_eq!(r.analyzer.calls(), 3, "duplicates are not analysed");

    // The stored state and report match.
    assert_eq!(r.mgr.job_state(job).unwrap(), JobState::Completed);
    let report = r.mgr.job_report(job).unwrap();
    assert_eq!(report.len(), 6);
    assert_eq!(
        report
            .iter()
            .filter(|i| i.status == JobItemStatus::Done)
            .count(),
        3
    );
    assert!(report.iter().all(|i| i.track.is_some()));
    let jobs = r.mgr.jobs().unwrap();
    assert_eq!((jobs[0].id, jobs[0].items_done), (job, 3));
    assert!(r.mgr.active_jobs().is_empty());
}

#[test]
fn importing_the_same_folder_again_finds_only_duplicates() {
    let dir = tmp("again");
    make_wavs(&dir, 4);
    let r = rig(Duration::ZERO, 2);
    let first = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    wait_terminal(&r.rx, first, WAIT);
    let second = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    let end = wait_terminal(&r.rx, second, WAIT);
    let end = last(&end);
    assert_eq!((end.items_done, end.items_duplicate), (0, 4));
    assert_eq!(r.lib.track_count().unwrap(), 4);
    assert_eq!(r.analyzer.calls(), 4);
}

#[test]
fn a_moved_file_relinks_instead_of_duplicating() {
    let dir = tmp("moved");
    let files = make_wavs(&dir.join("old"), 1);
    let r = rig(Duration::ZERO, 1);
    let j = r
        .mgr
        .import_folder(&dir.join("old"), ImportOptions::default())
        .unwrap();
    wait_terminal(&r.rx, j, WAIT);
    let id = all_tracks(&r.lib)[0].id;
    r.lib.set_rating(id, 5).unwrap();

    std::fs::create_dir_all(dir.join("new")).unwrap();
    std::fs::rename(&files[0], dir.join("new/renamed.wav")).unwrap();
    let j = r
        .mgr
        .import_folder(&dir.join("new"), ImportOptions::default())
        .unwrap();
    let end = wait_terminal(&r.rx, j, WAIT);
    assert_eq!((last(&end).items_done, last(&end).items_duplicate), (0, 1));
    let tracks = all_tracks(&r.lib);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].id, id, "same track, new path");
    assert_eq!(
        tracks[0].path,
        normalize_path_string(&dir.join("new/renamed.wav"))
    );
    assert_eq!(tracks[0].rating, 5, "user data kept");
}

#[test]
fn same_size_head_and_tail_still_needs_a_full_hash_to_be_a_duplicate() {
    let dir = tmp("collision");
    // About 2.6 MB: larger than the 2 MiB the head and tail hashes cover.
    write_wav(&dir.join("a.wav"), 44_100, 30.0, 220.0);
    let mut bytes = std::fs::read(dir.join("a.wav")).unwrap();
    assert!(bytes.len() > 2 * 1024 * 1024 + 4096);
    let mid = bytes.len() / 2;
    // Same size, same first and last MiB, different middle.
    bytes[mid] ^= 0x55;
    std::fs::write(dir.join("b.wav"), &bytes).unwrap();
    std::fs::copy(dir.join("a.wav"), dir.join("c.wav")).unwrap();

    let r = rig(Duration::ZERO, 1);
    let j = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    let end = wait_terminal(&r.rx, j, WAIT);
    let end = last(&end);
    assert_eq!(end.items_done, 2, "a and b are different files");
    assert_eq!(end.items_duplicate, 1, "c equals a");
    assert_eq!(r.lib.track_count().unwrap(), 2);
}

#[test]
fn undecodable_files_fail_without_stopping_the_job() {
    let dir = tmp("corrupt");
    make_wavs(&dir, 2);
    std::fs::write(dir.join("zz-broken.wav"), vec![0x13u8; 4096]).unwrap();
    std::fs::write(dir.join("zz-empty.mp3"), b"").unwrap();
    let r = rig(Duration::ZERO, 2);
    let j = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    let end = wait_terminal(&r.rx, j, WAIT);
    let end = last(&end);
    assert_eq!(end.state, JobState::Completed);
    assert_eq!((end.items_done, end.items_failed), (2, 2));
    let report = r.mgr.job_report(j).unwrap();
    let failed: Vec<_> = report
        .iter()
        .filter(|i| i.status == JobItemStatus::Failed)
        .collect();
    assert_eq!(failed.len(), 2);
    assert!(
        failed
            .iter()
            .all(|i| i.error.as_deref().is_some_and(|e| !e.is_empty()))
    );
    // The broken track stays in the library, flagged failed, so the user can see and retry it.
    let broken = all_tracks(&r.lib)
        .into_iter()
        .find(|t| t.path.ends_with("zz-broken.wav"))
        .unwrap();
    assert_eq!(broken.analysis_status, AnalysisStatus::Failed);
}

#[test]
fn files_import_reports_missing_and_unsupported() {
    let dir = tmp("files");
    let wavs = make_wavs(&dir, 2);
    std::fs::write(dir.join("readme.txt"), "x").unwrap();
    let r = rig(Duration::ZERO, 2);
    let j = r
        .mgr
        .import_files(
            &[
                wavs[0].clone(),
                dir.join("gone.wav"),
                dir.join("readme.txt"),
                wavs[1].clone(),
            ],
            ImportOptions::default(),
        )
        .unwrap();
    let end = wait_terminal(&r.rx, j, WAIT);
    let end = last(&end);
    assert_eq!(
        (end.items_total, end.items_done, end.items_failed),
        (4, 2, 2)
    );
    let report = r.mgr.job_report(j).unwrap();
    assert_eq!(report[1].error.as_deref(), Some("file not found"));
    assert_eq!(report[2].error.as_deref(), Some("unsupported format"));
}

#[test]
fn folder_import_fills_the_target_playlist() {
    let dir = tmp("target");
    make_wavs(&dir, 3);
    let lib = Library::open_in_memory().unwrap();
    let pl = lib.create_playlist(None, "Inbox").unwrap();
    let r = rig_with(lib, Duration::ZERO, 2);
    let j = r
        .mgr
        .import_folder(
            &dir,
            ImportOptions {
                target_playlist: Some(pl),
                ..ImportOptions::default()
            },
        )
        .unwrap();
    wait_terminal(&r.rx, j, WAIT);
    assert_eq!(r.lib.playlist_track_ids(pl).unwrap().len(), 3);
}

#[test]
fn analysis_can_be_limited_by_features() {
    let dir = tmp("features");
    make_wavs(&dir, 1);
    let r = rig(Duration::ZERO, 1);
    // The fake provides all features; asking for none runs no analyzer.
    let none = AnalysisFeatures::default();
    let j = r
        .mgr
        .import_folder(
            &dir,
            ImportOptions {
                features: none,
                ..ImportOptions::default()
            },
        )
        .unwrap();
    wait_terminal(&r.rx, j, WAIT);
    let t = &all_tracks(&r.lib)[0];
    assert_eq!(t.analysis_status, AnalysisStatus::Pending);
    assert_eq!(r.analyzer.calls(), 0);
    assert!(r.lib.get_analysis(t.id).unwrap().is_none());
}

// ---- responsiveness, cancel, pause, resume ----

#[test]
fn starting_a_job_and_querying_never_wait_for_the_workers() {
    let dir = tmp("responsive");
    make_wavs(&dir, 24);
    let r = rig(Duration::from_millis(150), 2);
    let t = Instant::now();
    let job = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    let started = t.elapsed();
    assert!(
        started < Duration::from_millis(100),
        "import_folder blocked for {started:?}"
    );

    // While workers decode and analyse, the collection view answers immediately.
    let mut worst = Duration::ZERO;
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        let t = Instant::now();
        r.lib.query_tracks(&TrackQuery::default()).unwrap();
        worst = worst.max(t.elapsed());
        std::thread::sleep(Duration::from_millis(10));
    }
    eprintln!("start {started:?}, worst query during import {worst:?}");
    assert!(
        worst < Duration::from_millis(50),
        "query blocked for {worst:?}"
    );
    r.mgr.cancel(job).unwrap();
    wait_terminal(&r.rx, job, WAIT);
}

#[test]
fn cancel_stops_workers_within_100_ms() {
    let dir = tmp("cancel");
    make_wavs(&dir, 10);
    // 5 s per track: without cancel this would take far longer than the test.
    let r = rig(Duration::from_secs(5), 4);
    let job = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    let deadline = Instant::now() + WAIT;
    while r.analyzer.calls() < 4 {
        assert!(
            Instant::now() < deadline,
            "workers never reached the analyzer"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let t = Instant::now();
    r.mgr.cancel(job).unwrap();
    let events = wait_terminal(&r.rx, job, WAIT);
    let took = t.elapsed();
    eprintln!("cancel to terminal event: {took:?}");
    assert!(took < Duration::from_millis(100), "cancel took {took:?}");
    assert_eq!(last(&events).state, JobState::Cancelled);
    assert_eq!(r.mgr.job_state(job).unwrap(), JobState::Cancelled);
    assert!(r.mgr.active_jobs().is_empty());

    // Interrupted tracks are not left looking busy, and no item is wrongly finished.
    for t in all_tracks(&r.lib) {
        assert_eq!(t.analysis_status, AnalysisStatus::Pending, "{}", t.path);
    }
    assert!(
        r.mgr
            .job_report(job)
            .unwrap()
            .iter()
            .all(|i| i.status == JobItemStatus::Pending)
    );
    // A cancelled job is terminal: it does not come back on restart.
    assert!(r.mgr.resume_pending().unwrap().is_empty());
    assert!(r.mgr.resume(job).is_err());
}

#[test]
fn pause_then_resume_finishes_the_remaining_items() {
    let dir = tmp("pause");
    make_wavs(&dir, 8);
    let r = rig(Duration::from_millis(120), 2);
    let job = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    while r.analyzer.calls() < 2 {
        std::thread::sleep(Duration::from_millis(5));
    }
    r.mgr.pause(job).unwrap();
    let events = wait_terminal(&r.rx, job, WAIT);
    assert_eq!(last(&events).state, JobState::Paused);
    let done_before = last(&events).items_done;
    assert!(done_before < 8);
    assert_eq!(r.mgr.job_state(job).unwrap(), JobState::Paused);
    assert!(
        r.mgr.resume_pending().unwrap().is_empty(),
        "paused jobs stay paused after a restart"
    );

    r.mgr.resume(job).unwrap();
    let events = wait_terminal(&r.rx, job, WAIT);
    let end = last(&events);
    assert_eq!(end.state, JobState::Completed);
    assert_eq!((end.items_total, end.items_done), (8, 8));
    assert_eq!(
        all_tracks(&r.lib)
            .iter()
            .filter(|t| t.analysis_status == AnalysisStatus::Done)
            .count(),
        8
    );
}

#[test]
fn a_job_interrupted_by_quitting_resumes_from_unfinished_items() {
    let dir = tmp("resume");
    let music = dir.join("music");
    make_wavs(&music, 12);
    let db = dir.join(DATABASE_FILE_NAME);

    // First run: quit while the job is in flight, as a crash or app exit would.
    let job: JobId;
    let calls_first;
    {
        let r = rig_with(Library::open(&db).unwrap(), Duration::from_millis(80), 2);
        job = r
            .mgr
            .import_folder(&music, ImportOptions::default())
            .unwrap();
        while r.analyzer.calls() < 4 {
            std::thread::sleep(Duration::from_millis(5));
        }
        let t = Instant::now();
        drop(r.mgr); // stops the workers; the job row is left as it was
        eprintln!("shutdown took {:?}", t.elapsed());
        calls_first = r.analyzer.calls();
    }

    // Second run: a fresh process opens the same file.
    let lib = Library::open(&db).unwrap();
    let stored = lib.track_count().unwrap();
    assert!(
        (4..=12).contains(&stored),
        "tracks are visible while the job runs: {stored}"
    );
    let r = rig_with(lib, Duration::ZERO, 2);
    assert_eq!(
        r.mgr.job_state(job).unwrap(),
        JobState::Running,
        "left running by the first run"
    );
    let finished_before = r
        .mgr
        .job_report(job)
        .unwrap()
        .iter()
        .filter(|i| i.status == JobItemStatus::Done)
        .count();
    assert!(finished_before < 12);

    assert_eq!(r.mgr.resume_pending().unwrap(), vec![job]);
    let events = wait_terminal(&r.rx, job, WAIT);
    let end = last(&events);
    assert_eq!(end.state, JobState::Completed);
    assert_eq!(
        (end.items_total, end.items_done, end.items_failed),
        (12, 12, 0)
    );
    assert_eq!(
        end.items_duplicate, 0,
        "resumed placeholder rows are not mistaken for duplicates"
    );
    assert_eq!(r.lib.track_count().unwrap(), 12);
    for t in all_tracks(&r.lib) {
        assert_eq!(t.analysis_status, AnalysisStatus::Done);
        assert!(r.lib.get_analysis(t.id).unwrap().is_some());
    }
    // Finished items are not redone: the second run analyses only what was left.
    let calls_second = r.analyzer.calls();
    assert_eq!(calls_second, 12 - finished_before);
    eprintln!(
        "first run analysed {calls_first} (finished {finished_before}), second run {calls_second}"
    );
    assert!(r.mgr.resume_pending().unwrap().is_empty());
}

#[test]
fn a_new_file_in_the_folder_is_picked_up_when_a_job_resumes() {
    let dir = tmp("resume-new");
    let music = dir.join("music");
    make_wavs(&music, 4);
    let db = dir.join(DATABASE_FILE_NAME);
    let job;
    {
        let r = rig_with(Library::open(&db).unwrap(), Duration::from_millis(200), 1);
        job = r
            .mgr
            .import_folder(&music, ImportOptions::default())
            .unwrap();
        while r.analyzer.calls() < 1 {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    write_wav(&music.join("track-99.wav"), 22_050, 0.5, 999.0);
    let r = rig_with(Library::open(&db).unwrap(), Duration::ZERO, 2);
    r.mgr.resume_pending().unwrap();
    let end = wait_terminal(&r.rx, job, WAIT);
    assert_eq!(last(&end).items_total, 5);
    assert_eq!(r.lib.track_count().unwrap(), 5);
}

// ---- re-analysis ----

#[test]
fn reanalysis_replaces_results_but_keeps_a_locked_grid() {
    let dir = tmp("reanalyze");
    make_wavs(&dir, 2);
    let lib = Library::open_in_memory().unwrap();
    let r = rig_with(lib, Duration::ZERO, 2);
    let j = r.mgr.import_folder(&dir, ImportOptions::default()).unwrap();
    wait_terminal(&r.rx, j, WAIT);
    let tracks = all_tracks(&r.lib);
    let (locked, free) = (tracks[0].id, tracks[1].id);
    let mine = BeatGrid::new(Seconds(0.33), Bpm(99.0), 4).unwrap();
    r.lib.set_beat_grid(locked, &mine, true).unwrap();
    r.lib.mark_stale("fake", 2).unwrap();
    assert_eq!(
        r.lib.analysis_status(free).unwrap(),
        Some(AnalysisStatus::Stale)
    );
    assert_eq!(r.lib.tracks_needing_analysis("fake", 2).unwrap().len(), 2);

    // A newer analyzer version, run over both tracks as one job.
    let mut newer = FakeAnalyzer::new(Duration::ZERO);
    newer.version = 2;
    let newer = Arc::new(newer);
    let a: Arc<dyn Analyzer> = newer.clone();
    let (mgr2, rx2) = ImportManager::new(r.lib.clone(), vec![a], config(2));
    let j = mgr2
        .reanalyze(&[locked, free], AnalysisFeatures::ALL)
        .unwrap();
    let end = wait_terminal(&rx2, j, WAIT);
    let end = last(&end);
    assert_eq!(
        (end.kind, end.state, end.items_done),
        (JobKind::Reanalyze, JobState::Completed, 2)
    );
    assert_eq!(newer.calls(), 2);

    let a = r.lib.get_analysis(locked).unwrap().unwrap();
    assert_eq!(a.analyzer_version, 2);
    assert_eq!(
        (a.beat_grid, a.bpm, a.grid_locked),
        (Some(mine), Some(99.0), true),
        "locked grid kept"
    );
    let a = r.lib.get_analysis(free).unwrap().unwrap();
    assert_eq!((a.analyzer_version, a.bpm), (2, Some(120.0)));
    assert_eq!(
        r.lib.analysis_status(free).unwrap(),
        Some(AnalysisStatus::Done)
    );
    assert!(r.lib.tracks_needing_analysis("fake", 2).unwrap().is_empty());

    // One track on its own, for a file that vanished: reported, flagged missing, not fatal.
    std::fs::remove_file(&tracks[1].path).unwrap();
    let j = mgr2.reanalyze_track(free, AnalysisFeatures::ALL).unwrap();
    let end = wait_terminal(&rx2, j, WAIT);
    assert_eq!(
        (last(&end).state, last(&end).items_failed),
        (JobState::Completed, 1)
    );
    assert!(r.lib.get_track(free).unwrap().unwrap().missing);
    assert_eq!(
        mgr2.job_report(j).unwrap()[0].error.as_deref(),
        Some("file not found")
    );
}

// ---- playlists and rekordbox ----

#[test]
fn m3u_import_builds_an_ordered_playlist() {
    let dir = tmp("m3u");
    let wavs = make_wavs(&dir.join("audio"), 3);
    let list = dir.join("Friday Warmup.m3u8");
    std::fs::write(
        &list,
        format!(
            "#EXTM3U\n#EXTINF:1,x\naudio/track-02.wav\n{}\naudio/missing.wav\naudio/track-02.wav\naudio/track-00.wav\n",
            wavs[1].display()
        ),
    )
    .unwrap();
    let r = rig(Duration::ZERO, 2);
    let j = r
        .mgr
        .import_playlist(&list, ImportOptions::default())
        .unwrap();
    let events = wait_terminal(&r.rx, j, WAIT);
    let end = last(&events);
    assert_eq!(end.state, JobState::Completed);
    assert_eq!(
        (end.items_total, end.items_done, end.items_failed),
        (4, 3, 1)
    );
    assert_eq!(r.lib.track_count().unwrap(), 3);

    let tree = r.lib.playlist_tree().unwrap();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0].name, "Friday Warmup");
    let titles: Vec<_> = r
        .lib
        .playlist_entries(tree[0].id)
        .unwrap()
        .into_iter()
        .map(|e| e.track.title.unwrap())
        .collect();
    // File order, repeats kept, the missing file skipped.
    assert_eq!(titles, ["track-02", "track-01", "track-02", "track-00"]);
    let missing = r
        .mgr
        .job_report(j)
        .unwrap()
        .into_iter()
        .find(|i| i.status == JobItemStatus::Failed)
        .unwrap();
    assert_eq!(missing.error.as_deref(), Some("file not found"));
    assert!(missing.path.ends_with("audio/missing.wav"));
}

#[test]
fn rekordbox_xml_maps_tracks_grids_cues_and_playlists() {
    let dir = tmp("rekordbox");
    write_wav(&dir.join("a.wav"), 22_050, 0.5, 300.0);
    write_wav(&dir.join("b.wav"), 22_050, 0.5, 400.0);
    write_wav(&dir.join("sub dir/c d.wav"), 22_050, 0.5, 500.0);
    let xml =
        include_str!("fixtures/rekordbox_sample.xml").replace("{{DIR}}", &dir.to_string_lossy());
    let xml_path = dir.join("collection.xml");
    std::fs::write(&xml_path, xml).unwrap();

    let r = rig(Duration::ZERO, 2);
    let j = r
        .mgr
        .import_rekordbox_xml(&xml_path, ImportOptions::default())
        .unwrap();
    let events = wait_terminal(&r.rx, j, WAIT);
    let end = last(&events);
    assert_eq!(end.state, JobState::Completed);
    assert_eq!(
        (end.items_total, end.items_done, end.items_failed),
        (4, 3, 1)
    );

    // The track that is not on disk is reported, not fatal.
    let report = r.mgr.job_report(j).unwrap();
    let ghost = report
        .iter()
        .find(|i| i.status == JobItemStatus::Failed)
        .unwrap();
    assert!(ghost.path.ends_with("missing.wav"));
    assert_eq!(ghost.error.as_deref(), Some("file not found"));

    let by_path = |name: &str| {
        all_tracks(&r.lib)
            .into_iter()
            .find(|t| t.path.ends_with(name))
            .unwrap()
    };
    let (a, b, c) = (by_path("a.wav"), by_path("b.wav"), by_path("c d.wav"));
    assert!(
        c.path.contains("sub dir"),
        "percent-encoded location decoded"
    );

    // Metadata from the export fills what the files' tags left empty.
    let ta = r.lib.get_track(a.id).unwrap().unwrap();
    assert_eq!(ta.meta.title.as_deref(), Some("Alpha & Omega"));
    assert_eq!(ta.meta.artist.as_deref(), Some("Test Artist"));
    assert_eq!(ta.meta.genre.as_deref(), Some("House"));
    assert_eq!(ta.meta.year, Some(2020));
    assert_eq!(ta.meta.tag_key.as_deref(), Some("Am"));
    assert_eq!(ta.rating, 5, "255 maps to 5 stars");
    assert_eq!(
        r.lib.get_track(b.id).unwrap().unwrap().rating,
        2,
        "102 maps to 2 stars"
    );

    // TEMPO becomes a locked constant grid.
    let ga = r.lib.get_analysis(a.id).unwrap().unwrap();
    assert!(ga.grid_locked);
    let grid = ga.beat_grid.unwrap();
    assert_eq!(
        (grid.anchor(), grid.bpm(), grid.beats_per_bar()),
        (Seconds(0.1), Bpm(120.0), 4)
    );
    assert_eq!(ga.bpm, Some(120.0));
    // Variable tempo keeps the first segment; Battito 3 moves the anchor back two beats to the downbeat.
    let gb = r
        .lib
        .get_analysis(b.id)
        .unwrap()
        .unwrap()
        .beat_grid
        .unwrap();
    assert_eq!(gb.bpm(), Bpm(128.0));
    assert!(
        (gb.anchor().0 - (1.0 - 2.0 * 60.0 / 128.0)).abs() < 1e-9,
        "{:?}",
        gb.anchor()
    );
    // The fake analyzer ran first; the curated grid replaced its grid but other results stay.
    assert_eq!(
        r.lib
            .get_analysis(a.id)
            .unwrap()
            .unwrap()
            .key
            .unwrap()
            .camelot(),
        "8A"
    );

    // POSITION_MARK becomes cues: memory, hot slot 0 with colour, a hot loop in slot 2; the fade is skipped.
    let cues = r.lib.cues(a.id).unwrap();
    assert_eq!(cues.len(), 3);
    assert_eq!(
        (cues[0].kind, cues[0].start, cues[0].name.as_str()),
        (CueKind::Memory, Seconds(1.5), "Intro")
    );
    let drop = &cues[1];
    assert!(matches!(drop.kind, CueKind::Hot { index } if index.index() == 0));
    assert_eq!(
        (drop.start, drop.color.map(|c| (c.r, c.g, c.b))),
        (Seconds(2.0), Some((40, 226, 20)))
    );
    let looped = &cues[2];
    assert!(matches!(looped.kind, CueKind::Hot { index } if index.index() == 2));
    assert_eq!(
        (looped.start, looped.end),
        (Seconds(4.0), Some(Seconds(8.0)))
    );
    assert!(r.lib.cues(b.id).unwrap().is_empty());
    assert!(
        r.lib.cues(c.id).unwrap().is_empty(),
        "slot 9 does not exist on our pads"
    );

    // PLAYLISTS/NODE tree, under a folder named after the file.
    let tree = r.lib.playlist_tree().unwrap();
    let node = |name: &str| tree.iter().find(|n| n.name == name).unwrap().clone();
    let root = node("collection");
    assert_eq!(node("Sets").parent, Some(root.id));
    assert_eq!(node("Warmup").parent, Some(node("Sets").id));
    assert_eq!(node("Peak").parent, Some(root.id));
    let titles = |name: &str| -> Vec<String> {
        r.lib
            .playlist_entries(node(name).id)
            .unwrap()
            .into_iter()
            .map(|e| e.track.title.unwrap())
            .collect()
    };
    // The entry for the missing track is skipped; order is the export's.
    assert_eq!(titles("Warmup"), ["Alpha & Omega", "Gamma Spaced", "Beta"]);
    assert_eq!(
        titles("Peak"),
        ["Beta", "Alpha & Omega"],
        "KeyType 1 refers to locations"
    );

    // Importing the same export again changes nothing in cues, and adds only a new folder.
    let j2 = r
        .mgr
        .import_rekordbox_xml(&xml_path, ImportOptions::default())
        .unwrap();
    let end = wait_terminal(&r.rx, j2, WAIT);
    assert_eq!((last(&end).items_done, last(&end).items_duplicate), (0, 3));
    assert_eq!(r.lib.cues(a.id).unwrap().len(), 3);
    assert_eq!(r.lib.track_count().unwrap(), 3);
}

#[test]
fn rekordbox_import_does_not_override_a_grid_the_user_locked() {
    let dir = tmp("rekordbox-lock");
    write_wav(&dir.join("a.wav"), 22_050, 0.5, 300.0);
    let r = rig(Duration::ZERO, 1);
    let j = r
        .mgr
        .import_files(&[dir.join("a.wav")], ImportOptions::default())
        .unwrap();
    wait_terminal(&r.rx, j, WAIT);
    let id = all_tracks(&r.lib)[0].id;
    let mine = BeatGrid::new(Seconds(0.5), Bpm(111.0), 4).unwrap();
    r.lib.set_beat_grid(id, &mine, true).unwrap();

    let xml = r#"<DJ_PLAYLISTS Version="1.0.0"><COLLECTION Entries="1">
        <TRACK TrackID="1" Name="A" Location="file://localhost{{DIR}}/a.wav">
        <TEMPO Inizio="0.0" Bpm="140.00" Metro="4/4" Battito="1"/></TRACK></COLLECTION></DJ_PLAYLISTS>"#
        .replace("{{DIR}}", &dir.to_string_lossy());
    std::fs::write(dir.join("x.xml"), xml).unwrap();
    let j = r
        .mgr
        .import_rekordbox_xml(&dir.join("x.xml"), ImportOptions::default())
        .unwrap();
    wait_terminal(&r.rx, j, WAIT);
    let a = r.lib.get_analysis(id).unwrap().unwrap();
    assert_eq!(a.beat_grid, Some(mine));
}

#[test]
fn a_broken_source_file_fails_the_job_with_a_message() {
    let dir = tmp("badxml");
    std::fs::write(dir.join("bad.xml"), "<html><body/></html>").unwrap();
    let r = rig(Duration::ZERO, 1);
    let j = r
        .mgr
        .import_rekordbox_xml(&dir.join("bad.xml"), ImportOptions::default())
        .unwrap();
    let events = wait_terminal(&r.rx, j, WAIT);
    assert_eq!(last(&events).state, JobState::Failed);
    let summary = r
        .mgr
        .jobs()
        .unwrap()
        .into_iter()
        .find(|s| s.id == j)
        .unwrap();
    assert_eq!(summary.state, JobState::Failed);
    assert!(summary.error.unwrap().contains("DJ_PLAYLISTS"));

    let j = r
        .mgr
        .import_folder(&dir.join("does-not-exist"), ImportOptions::default())
        .unwrap();
    let events = wait_terminal(&r.rx, j, WAIT);
    assert_eq!(last(&events).state, JobState::Failed);
}
