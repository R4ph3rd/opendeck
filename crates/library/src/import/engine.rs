//! Worker pool, job runner and the per-item pipeline.
//!
//! Threads: `workers` pipeline threads share one work channel; each job also
//! gets a short-lived discover thread that walks the source, writes item rows
//! and feeds the channel. Callers never wait on any of them.
//!
//! Item stages are recorded in `import_job_items.stage` at two points that
//! matter for resume: `dedupe` (the track row exists, a placeholder with path,
//! size and fingerprint) and `tags` (tags and artwork written). Items finish
//! as `done`, `duplicate` or `failed` together with their analysis write, in
//! one transaction. Anything still `pending` after a crash re-runs from its
//! recorded stage.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use opendeck_contracts::ids::{JobId, PlaylistId, TrackId};
use opendeck_contracts::jobs::{JobItemStatus, JobKind, JobProgress, JobStage, JobState};
use opendeck_media::{MediaError, TrackTags, decode_file, read_tags};
use rusqlite::{OptionalExtension, Transaction, params};

use super::discover;
use super::hash::{fingerprint, full_hash};
use super::{ImportConfig, ItemReport, JobSummary, default_workers};
use crate::analysis::{save_analysis_on, set_beat_grid_on};
use crate::cues::add_cue_on;
use crate::error::{LibraryError, Result};
use crate::library::{Library, now_ms};
use crate::paths::{normalize_path, normalize_path_string};
use crate::playlists::{add_tracks_on, create_folder_on, create_playlist_on};
use crate::rekordbox::{self, RbEntry, RbNode};
use crate::tracks::{insert_track_on, store_artwork_on, tracks_by_fingerprint_on};
use crate::types::{AnalyzerStamp, NewTrack, TrackMeta, TrackTech};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---- string mappings for the CHECK-constrained columns ----

pub(crate) fn kind_str(k: JobKind) -> &'static str {
    match k {
        JobKind::ImportFolder => "import_folder",
        JobKind::ImportFiles => "import_files",
        JobKind::ImportPlaylist => "import_playlist",
        JobKind::ImportRekordboxXml => "import_rekordbox_xml",
        JobKind::Reanalyze => "reanalyze",
    }
}

fn kind_parse(s: &str) -> Option<JobKind> {
    Some(match s {
        "import_folder" => JobKind::ImportFolder,
        "import_files" => JobKind::ImportFiles,
        "import_playlist" => JobKind::ImportPlaylist,
        "import_rekordbox_xml" => JobKind::ImportRekordboxXml,
        "reanalyze" => JobKind::Reanalyze,
        _ => return None,
    })
}

fn state_str(s: JobState) -> &'static str {
    match s {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Paused => "paused",
        JobState::Cancelled => "cancelled",
        JobState::Completed => "completed",
        JobState::Failed => "failed",
    }
}

fn state_parse(s: &str) -> Option<JobState> {
    Some(match s {
        "queued" => JobState::Queued,
        "running" => JobState::Running,
        "paused" => JobState::Paused,
        "cancelled" => JobState::Cancelled,
        "completed" => JobState::Completed,
        "failed" => JobState::Failed,
        _ => return None,
    })
}

fn stage_parse(s: &str) -> Option<JobStage> {
    Some(match s {
        "discover" => JobStage::Discover,
        "dedupe" => JobStage::Dedupe,
        "tags" => JobStage::Tags,
        "decode" => JobStage::Decode,
        "analyze" => JobStage::Analyze,
        "persist" => JobStage::Persist,
        _ => return None,
    })
}

fn item_status_parse(s: &str) -> JobItemStatus {
    match s {
        "done" => JobItemStatus::Done,
        "duplicate" => JobItemStatus::Duplicate,
        "failed" => JobItemStatus::Failed,
        _ => JobItemStatus::Pending,
    }
}

pub(crate) fn features_to_json(f: AnalysisFeatures) -> serde_json::Value {
    serde_json::json!({ "tempo": f.tempo, "key": f.key, "loudness": f.loudness, "peaks": f.peaks })
}

fn features_from_json(options: &str) -> AnalysisFeatures {
    let v: serde_json::Value = serde_json::from_str(options).unwrap_or_default();
    let f = &v["features"];
    if f.is_null() {
        return AnalysisFeatures::ALL;
    }
    let b = |k: &str| f[k].as_bool().unwrap_or(true);
    AnalysisFeatures {
        tempo: b("tempo"),
        key: b("key"),
        loudness: b("loudness"),
        peaks: b("peaks"),
    }
}

fn overlaps(a: AnalysisFeatures, b: AnalysisFeatures) -> bool {
    (a.tempo && b.tempo) || (a.key && b.key) || (a.loudness && b.loudness) || (a.peaks && b.peaks)
}

// ---- runtime state ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stop {
    Cancel,
    Pause,
    Shutdown,
}

struct Counters {
    state: JobState,
    total: u32,
    done: u32,
    duplicate: u32,
    failed: u32,
    /// Items enqueued or about to be, not yet finished or skipped.
    remaining: u32,
    discovered: bool,
    finished: bool,
    stage: Option<JobStage>,
    current: Option<String>,
    last_track: Option<TrackId>,
    last_emit: Option<Instant>,
    error: Option<String>,
}

pub(crate) struct JobRuntime {
    id: JobId,
    kind: JobKind,
    features: AnalysisFeatures,
    target_playlist: Option<PlaylistId>,
    source: Option<String>,
    cancel: CancelFlag,
    stop: Mutex<Option<Stop>>,
    counters: Mutex<Counters>,
}

impl JobRuntime {
    fn stopped(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub(crate) fn request_stop(&self, why: Stop) {
        let mut s = lock(&self.stop);
        if s.is_none() {
            *s = Some(why);
        }
        self.cancel.cancel();
    }
}

struct WorkItem {
    job: Arc<JobRuntime>,
    item_id: i64,
    path: PathBuf,
    track_id: Option<TrackId>,
    stage: Option<JobStage>,
}

enum Outcome {
    Done(TrackId),
    Duplicate(TrackId),
    Failed(Option<TrackId>, String),
    /// Stopped by cancel, pause or shutdown; the item stays pending.
    Interrupted(Option<TrackId>),
}

/// What discovery produces for one item.
pub(crate) struct NewItem {
    pub path: String,
    pub track_id: Option<TrackId>,
    /// Pre-failed (file missing, unsupported): stored as a failed item, never queued.
    pub failed: Option<String>,
}

/// Where a job's items come from.
pub(crate) enum Source {
    /// Items already stored (resume of files and re-analysis jobs).
    Stored,
    Folder(PathBuf),
    Files(Vec<NewItem>),
    Playlist(PathBuf),
    Rekordbox(PathBuf),
}

pub(crate) struct Shared {
    pub(crate) lib: Library,
    analyzers: Vec<Arc<dyn Analyzer>>,
    cfg: ImportConfig,
    work_tx: Sender<WorkItem>,
    work_rx: Receiver<WorkItem>,
    progress: Sender<JobProgress>,
    jobs: Mutex<HashMap<i64, Arc<JobRuntime>>>,
    /// Serialises dedupe check + placeholder insert so two identical files
    /// imported at once cannot both pass.
    dedupe_lock: Mutex<()>,
    shutdown: AtomicBool,
    helpers: Mutex<Vec<JoinHandle<()>>>,
}

/// Owns the worker threads; dropping it stops them.
pub(crate) struct Engine {
    pub(crate) shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

impl Engine {
    pub(crate) fn start(
        lib: Library,
        analyzers: Vec<Arc<dyn Analyzer>>,
        cfg: ImportConfig,
        progress: Sender<JobProgress>,
    ) -> Self {
        let (work_tx, work_rx) = crossbeam_channel::unbounded();
        let n = if cfg.workers > 0 {
            cfg.workers
        } else {
            default_workers(std::thread::available_parallelism().map_or(1, std::num::NonZero::get))
        };
        let shared = Arc::new(Shared {
            lib,
            analyzers,
            cfg,
            work_tx,
            work_rx,
            progress,
            jobs: Mutex::new(HashMap::new()),
            dedupe_lock: Mutex::new(()),
            shutdown: AtomicBool::new(false),
            helpers: Mutex::new(Vec::new()),
        });
        let workers = (0..n)
            .map(|i| {
                let s = Arc::clone(&shared);
                std::thread::Builder::new()
                    .name(format!("library-import-{i}"))
                    .spawn(move || s.worker_loop())
                    .expect("spawn import worker")
            })
            .collect();
        Self { shared, workers }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        for job in lock(&self.shared.jobs).values() {
            job.request_stop(Stop::Shutdown);
        }
        let helpers = std::mem::take(&mut *lock(&self.shared.helpers));
        for h in helpers {
            let _ = h.join();
        }
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

// ---- job table access ----

struct JobRow {
    kind: JobKind,
    state: JobState,
    source: Option<String>,
    options: String,
    target_playlist: Option<PlaylistId>,
}

fn load_job(lib: &Library, id: JobId) -> Result<JobRow> {
    lib.read(|c| {
        let row = c
            .query_row(
                "SELECT kind, state, source, options, target_playlist_id FROM import_jobs WHERE id = ?1",
                [id.0],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or(LibraryError::NotFound("job"))?;
        Ok(JobRow {
            kind: kind_parse(&row.0).ok_or_else(|| LibraryError::Corrupt(row.0.clone()))?,
            state: state_parse(&row.1).ok_or_else(|| LibraryError::Corrupt(row.1.clone()))?,
            source: row.2,
            options: row.3,
            target_playlist: row.4.map(PlaylistId),
        })
    })
}

pub(crate) fn create_job(
    lib: &Library,
    kind: JobKind,
    source: Option<&str>,
    features: AnalysisFeatures,
    target: Option<PlaylistId>,
) -> Result<JobId> {
    let options = serde_json::json!({ "features": features_to_json(features) }).to_string();
    lib.write(|c| {
        c.execute(
            "INSERT INTO import_jobs (kind, state, source, options, target_playlist_id, created_at) \
             VALUES (?1, 'queued', ?2, ?3, ?4, ?5)",
            params![kind_str(kind), source, options, target.map(|p| p.0), now_ms()],
        )?;
        Ok(JobId(c.last_insert_rowid()))
    })
}

fn set_job_state(lib: &Library, id: JobId, state: JobState, error: Option<&str>) -> Result<()> {
    lib.write(|c| {
        set_job_state_on(c, id, state, error)?;
        Ok(())
    })
}

fn set_job_state_on(
    c: &rusqlite::Connection,
    id: JobId,
    state: JobState,
    error: Option<&str>,
) -> Result<()> {
    let now = now_ms();
    c.execute(
        "UPDATE import_jobs SET state = ?2, error = ?3, \
         started_at = CASE WHEN ?2 = 'running' THEN COALESCE(started_at, ?4) ELSE started_at END, \
         finished_at = CASE WHEN ?2 IN ('cancelled', 'completed', 'failed') THEN ?4 ELSE NULL END \
         WHERE id = ?1",
        params![id.0, state_str(state), error, now],
    )?;
    Ok(())
}

pub(crate) fn list_jobs(lib: &Library) -> Result<Vec<JobSummary>> {
    lib.read(|c| {
        let mut stmt = c.prepare(
            "SELECT j.id, j.kind, j.state, j.source, j.error, j.created_at, \
             (SELECT count(*) FROM import_job_items i WHERE i.job_id = j.id), \
             (SELECT count(*) FROM import_job_items i WHERE i.job_id = j.id AND i.status = 'done'), \
             (SELECT count(*) FROM import_job_items i WHERE i.job_id = j.id AND i.status = 'duplicate'), \
             (SELECT count(*) FROM import_job_items i WHERE i.job_id = j.id AND i.status = 'failed') \
             FROM import_jobs j ORDER BY j.id DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let kind: String = r.get(1)?;
                let state: String = r.get(2)?;
                Ok(JobSummary {
                    id: JobId(r.get(0)?),
                    kind: kind_parse(&kind).unwrap_or(JobKind::ImportFiles),
                    state: state_parse(&state).unwrap_or(JobState::Failed),
                    source: r.get(3)?,
                    error: r.get(4)?,
                    created_at: r.get(5)?,
                    items_total: r.get(6)?,
                    items_done: r.get(7)?,
                    items_duplicate: r.get(8)?,
                    items_failed: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

pub(crate) fn job_report(lib: &Library, id: JobId) -> Result<Vec<ItemReport>> {
    lib.read(|c| {
        let mut stmt = c.prepare(
            "SELECT path, status, stage, track_id, error FROM import_job_items \
             WHERE job_id = ?1 ORDER BY position, id",
        )?;
        let rows = stmt
            .query_map([id.0], |r| {
                let status: String = r.get(1)?;
                let stage: Option<String> = r.get(2)?;
                Ok(ItemReport {
                    path: PathBuf::from(r.get::<_, String>(0)?),
                    status: item_status_parse(&status),
                    stage: stage.as_deref().and_then(stage_parse),
                    track: r.get::<_, Option<i64>>(3)?.map(TrackId),
                    error: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    })
}

/// Marks a non-active job cancelled in the database (not running in this process).
pub(crate) fn cancel_stored(lib: &Library, id: JobId) -> Result<()> {
    let row = load_job(lib, id)?;
    if !row.state.is_terminal() {
        set_job_state(lib, id, JobState::Cancelled, None)?;
    }
    Ok(())
}

impl Shared {
    // ---- starting jobs ----

    /// Builds the runtime for a stored job and starts its discover thread.
    pub(crate) fn launch(self: &Arc<Self>, id: JobId, source: Source) -> Result<()> {
        if lock(&self.jobs).contains_key(&id.0) {
            return Err(LibraryError::Invalid("job is already running".into()));
        }
        let row = load_job(&self.lib, id)?;
        if row.state.is_terminal() {
            return Err(LibraryError::Invalid("job is finished".into()));
        }
        let (total, done, duplicate, failed) = self.lib.read(|c| {
            Ok(c.query_row(
                "SELECT count(*), \
                 coalesce(sum(status = 'done'), 0), coalesce(sum(status = 'duplicate'), 0), \
                 coalesce(sum(status = 'failed'), 0) FROM import_job_items WHERE job_id = ?1",
                [id.0],
                |r| {
                    Ok((
                        r.get::<_, u32>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, u32>(2)?,
                        r.get::<_, u32>(3)?,
                    ))
                },
            )?)
        })?;
        let job = Arc::new(JobRuntime {
            id,
            kind: row.kind,
            features: features_from_json(&row.options),
            target_playlist: row.target_playlist,
            source: row.source,
            cancel: CancelFlag::new(),
            stop: Mutex::new(None),
            counters: Mutex::new(Counters {
                state: JobState::Queued,
                total,
                done,
                duplicate,
                failed,
                remaining: 0,
                discovered: false,
                finished: false,
                stage: None,
                current: None,
                last_track: None,
                last_emit: None,
                error: None,
            }),
        });
        lock(&self.jobs).insert(id.0, Arc::clone(&job));
        let me = Arc::clone(self);
        let handle = std::thread::Builder::new()
            .name(format!("library-job-{}", id.0))
            .spawn(move || me.run_job(&job, source))
            .map_err(LibraryError::Io)?;
        let mut helpers = lock(&self.helpers);
        helpers.retain(|h| !h.is_finished());
        helpers.push(handle);
        Ok(())
    }

    pub(crate) fn resume_source(&self, id: JobId) -> Result<Source> {
        let row = load_job(&self.lib, id)?;
        let src = || {
            row.source
                .clone()
                .map(PathBuf::from)
                .ok_or_else(|| LibraryError::Corrupt("job has no source".into()))
        };
        Ok(match row.kind {
            JobKind::ImportFolder => Source::Folder(src()?),
            JobKind::ImportPlaylist => Source::Playlist(src()?),
            JobKind::ImportRekordboxXml => Source::Rekordbox(src()?),
            JobKind::ImportFiles | JobKind::Reanalyze => Source::Stored,
        })
    }

    pub(crate) fn job_state(&self, id: JobId) -> Result<JobState> {
        Ok(load_job(&self.lib, id)?.state)
    }

    pub(crate) fn active(&self, id: JobId) -> Option<Arc<JobRuntime>> {
        lock(&self.jobs).get(&id.0).cloned()
    }

    pub(crate) fn active_ids(&self) -> Vec<JobId> {
        lock(&self.jobs).keys().map(|k| JobId(*k)).collect()
    }

    pub(crate) fn resumable_ids(&self) -> Result<Vec<JobId>> {
        self.lib.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id FROM import_jobs WHERE state IN ('queued', 'running') ORDER BY id",
            )?;
            let rows = stmt
                .query_map([], |r| r.get(0).map(JobId))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    // ---- progress ----

    fn emit(&self, job: &JobRuntime, c: &mut Counters, force: bool) {
        let now = Instant::now();
        if !force
            && c.last_emit
                .is_some_and(|t| now.duration_since(t) < self.cfg.progress_interval)
        {
            return;
        }
        c.last_emit = Some(now);
        // A closed receiver just means nobody listens.
        let _ = self.progress.send(JobProgress {
            job: job.id,
            kind: job.kind,
            state: c.state,
            stage: c.stage,
            items_total: c.total,
            items_done: c.done,
            items_duplicate: c.duplicate,
            items_failed: c.failed,
            current_item: c.current.clone(),
            last_track: c.last_track,
        });
    }

    // ---- job runner ----

    fn run_job(self: &Arc<Self>, job: &Arc<JobRuntime>, source: Source) {
        if let Err(e) = self.run_job_inner(job, source) {
            lock(&job.counters).error = Some(e.to_string());
            job.request_stop(Stop::Cancel); // stop queued items; finalize reports failure
        }
        // Whatever happened, discovery is over; this may complete the job.
        {
            let mut c = lock(&job.counters);
            c.discovered = true;
        }
        self.maybe_finalize(job);
    }

    fn run_job_inner(self: &Arc<Self>, job: &Arc<JobRuntime>, source: Source) -> Result<()> {
        set_job_state(&self.lib, job.id, JobState::Running, None)?;
        {
            let mut c = lock(&job.counters);
            c.state = JobState::Running;
            c.stage = Some(JobStage::Discover);
            self.emit(job, &mut c, true);
        }
        // 1. Items left pending by an earlier run.
        let pending = self.lib.read(|c| {
            let mut stmt = c.prepare(
                "SELECT id, path, track_id, stage FROM import_job_items \
                 WHERE job_id = ?1 AND status = 'pending' ORDER BY position, id",
            )?;
            let rows = stmt
                .query_map([job.id.0], |r| {
                    let stage: Option<String> = r.get(3)?;
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        stage.as_deref().and_then(stage_parse),
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })?;
        for (item_id, path, track, stage) in pending {
            self.enqueue(job, item_id, PathBuf::from(path), track.map(TrackId), stage);
        }
        // 2. New items from the source.
        match source {
            Source::Stored => {}
            Source::Files(items) => self.add_items(job, items)?,
            Source::Folder(dir) => {
                let mut batch = Vec::new();
                let mut result = Ok(());
                discover::walk(
                    &dir,
                    &|| job.stopped() || self.shutdown.load(Ordering::Relaxed),
                    &mut |p| {
                        batch.push(NewItem {
                            path: normalize_path_string(&p),
                            track_id: None,
                            failed: None,
                        });
                        if batch.len() >= 256 && result.is_ok() {
                            result = self.add_items(job, std::mem::take(&mut batch));
                        }
                    },
                )?;
                result?;
                self.add_items(job, batch)?;
            }
            Source::Playlist(file) => {
                let paths = discover::parse_m3u(&file)?;
                self.add_items(job, paths.iter().map(|p| path_item(p)).collect())?;
            }
            Source::Rekordbox(file) => {
                let xml = fs::read_to_string(&file)?;
                let lib = rekordbox::parse(&xml)?;
                self.add_items(
                    job,
                    lib.tracks.iter().map(|t| path_item(&t.location)).collect(),
                )?;
            }
        }
        Ok(())
    }

    /// Inserts item rows (ignoring paths already in the job) and queues the new pending ones.
    fn add_items(self: &Arc<Self>, job: &Arc<JobRuntime>, items: Vec<NewItem>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let start_pos: i64 = self.lib.read(|c| {
            Ok(c.query_row(
                "SELECT count(*) FROM import_job_items WHERE job_id = ?1",
                [job.id.0],
                |r| r.get(0),
            )?)
        })?;
        let now = now_ms();
        // (item id, path, track id) of newly inserted pending rows; plus the count of new failed rows.
        let (queued, new_failed, new_total) = self.lib.write_tx(|tx| {
            let mut queued = Vec::new();
            let (mut failed, mut total) = (0u32, 0u32);
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO import_job_items (job_id, position, path, status, stage, track_id, error, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for (i, it) in items.into_iter().enumerate() {
                let (status, stage) = if it.failed.is_some() {
                    ("failed", Some("discover"))
                } else {
                    ("pending", None)
                };
                let n = stmt.execute(params![
                    job.id.0,
                    start_pos + i as i64,
                    it.path,
                    status,
                    stage,
                    it.track_id.map(|t| t.0),
                    it.failed,
                    now
                ])?;
                if n == 1 {
                    total += 1;
                    if it.failed.is_some() {
                        failed += 1;
                    } else {
                        queued.push((tx.last_insert_rowid(), it.path, it.track_id));
                    }
                }
            }
            Ok((queued, failed, total))
        })?;
        {
            let mut c = lock(&job.counters);
            c.total += new_total;
            c.failed += new_failed;
            self.emit(job, &mut c, false);
        }
        for (item_id, path, track) in queued {
            self.enqueue(job, item_id, PathBuf::from(path), track, None);
        }
        Ok(())
    }

    fn enqueue(
        &self,
        job: &Arc<JobRuntime>,
        item_id: i64,
        path: PathBuf,
        track_id: Option<TrackId>,
        stage: Option<JobStage>,
    ) {
        lock(&job.counters).remaining += 1;
        let _ = self.work_tx.send(WorkItem {
            job: Arc::clone(job),
            item_id,
            path,
            track_id,
            stage,
        });
    }

    fn maybe_finalize(self: &Arc<Self>, job: &Arc<JobRuntime>) {
        {
            let mut c = lock(&job.counters);
            if c.finished || !c.discovered || c.remaining > 0 {
                return;
            }
            c.finished = true;
        }
        self.finalize(job);
    }

    fn finalize(&self, job: &Arc<JobRuntime>) {
        let stop = *lock(&job.stop);
        let discover_error = lock(&job.counters).error.clone();
        let (state, error) = match (stop, discover_error) {
            (Some(Stop::Shutdown), _) => {
                // Leave the stored state as is: the job resumes on the next start.
                lock(&self.jobs).remove(&job.id.0);
                return;
            }
            (_, Some(e)) => (JobState::Failed, Some(e)),
            (Some(Stop::Cancel), None) => (JobState::Cancelled, None),
            (Some(Stop::Pause), None) => (JobState::Paused, None),
            (None, None) => match self.run_finalizer(job) {
                Ok(()) => (JobState::Completed, None),
                Err(e) => (JobState::Failed, Some(e.to_string())),
            },
        };
        if let Err(e) = set_job_state(&self.lib, job.id, state, error.as_deref()) {
            // The job row is unreachable; report it through progress and move on.
            let mut c = lock(&job.counters);
            c.error = Some(e.to_string());
        }
        // Leave the table before the final event, so a caller reacting to it can resume the job.
        lock(&self.jobs).remove(&job.id.0);
        let mut c = lock(&job.counters);
        c.state = state;
        if error.is_some() {
            c.error = error;
        }
        c.current = None;
        self.emit(job, &mut c, true);
    }

    // ---- workers ----

    fn worker_loop(self: &Arc<Self>) {
        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                return;
            }
            let item = match self.work_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(i) => i,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            let job = Arc::clone(&item.job);
            if job.stopped() {
                self.item_finished(&job, None);
                continue;
            }
            let outcome = self.process(&item);
            self.item_finished(&job, Some((&item, outcome)));
        }
    }

    /// Writes the item result and updates counters. `None` means the item was skipped.
    fn item_finished(
        self: &Arc<Self>,
        job: &Arc<JobRuntime>,
        result: Option<(&WorkItem, Outcome)>,
    ) {
        if let Some((item, outcome)) = result {
            let (status, track, error) = match &outcome {
                Outcome::Done(t) => (Some("done"), Some(*t), None),
                Outcome::Duplicate(t) => (Some("duplicate"), Some(*t), None),
                Outcome::Failed(t, e) => (Some("failed"), *t, Some(e.clone())),
                Outcome::Interrupted(t) => {
                    self.reset_interrupted(job, *t);
                    (None, *t, None)
                }
            };
            // Done items were already finalised with their analysis write.
            if let Some(status) = status
                && !matches!(outcome, Outcome::Done(_))
            {
                let _ = self.lib.write(|c| {
                    finish_item_on(c, item.item_id, status, track, error.as_deref())?;
                    if matches!(outcome, Outcome::Failed(Some(_), _)) {
                        c.execute(
                            "UPDATE tracks SET analysis_status = 'failed' WHERE id = ?1",
                            [track.map_or(0, |t| t.0)],
                        )?;
                    }
                    Ok(())
                });
            }
            let mut c = lock(&job.counters);
            match outcome {
                Outcome::Done(t) => {
                    c.done += 1;
                    c.last_track = Some(t);
                }
                Outcome::Duplicate(t) => {
                    c.duplicate += 1;
                    c.last_track = Some(t);
                }
                Outcome::Failed(..) => c.failed += 1,
                Outcome::Interrupted(_) => {}
            }
            c.remaining = c.remaining.saturating_sub(1);
            self.emit(job, &mut c, false);
        } else {
            let mut c = lock(&job.counters);
            c.remaining = c.remaining.saturating_sub(1);
        }
        self.maybe_finalize(job);
    }

    /// An interrupted analysis leaves the track pending (or stale if it had results).
    fn reset_interrupted(&self, job: &JobRuntime, track: Option<TrackId>) {
        let Some(t) = track else { return };
        let _ = self.lib.write(|c| {
            c.execute(
                "UPDATE tracks SET analysis_status = CASE \
                   WHEN EXISTS (SELECT 1 FROM analysis WHERE track_id = ?1) THEN 'stale' ELSE 'pending' END \
                 WHERE id = ?1 AND analysis_status = 'running'",
                [t.0],
            )?;
            Ok(())
        });
        let _ = job;
    }

    fn set_stage(&self, job: &JobRuntime, stage: JobStage, path: &Path) {
        let mut c = lock(&job.counters);
        c.stage = Some(stage);
        c.current = path.file_name().map(|n| n.to_string_lossy().into_owned());
        self.emit(job, &mut c, false);
    }

    // ---- the pipeline for one item ----

    fn process(&self, item: &WorkItem) -> Outcome {
        let job = &item.job;
        let path = item.path.as_path();
        let mut track = item.track_id;

        // Where to start: re-analysis and resumed items skip finished stages.
        let start =
            if job.kind == JobKind::Reanalyze || item.stage.is_some_and(|s| s >= JobStage::Tags) {
                JobStage::Decode
            } else if track.is_some() {
                JobStage::Tags
            } else {
                JobStage::Dedupe
            };

        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => {
                if job.kind == JobKind::Reanalyze
                    && let Some(t) = track
                {
                    let _ = self.lib.set_missing(t, true);
                }
                return Outcome::Failed(track, "file not found".into());
            }
        };

        if start <= JobStage::Dedupe {
            self.set_stage(job, JobStage::Dedupe, path);
            match self.dedupe_and_reserve(item, path, &meta) {
                Ok(Reserved::Duplicate(id)) => return Outcome::Duplicate(id),
                Ok(Reserved::New(id)) => track = Some(id),
                Err(e) => return Outcome::Failed(None, e.to_string()),
            }
        }
        let Some(track_id) = track else {
            return Outcome::Failed(None, "item has no track".into());
        };
        if job.stopped() {
            return Outcome::Interrupted(Some(track_id));
        }

        if start <= JobStage::Tags {
            self.set_stage(job, JobStage::Tags, path);
            if let Err(e) = self.write_tags(item, track_id, path) {
                return Outcome::Failed(Some(track_id), e.to_string());
            }
            if job.stopped() {
                return Outcome::Interrupted(Some(track_id));
            }
        }

        let active: Vec<&Arc<dyn Analyzer>> = self
            .analyzers
            .iter()
            .filter(|a| overlaps(a.features(), job.features))
            .collect();
        if active.is_empty() {
            // Nothing to analyse: the track is imported, analysis stays pending.
            return match self
                .lib
                .write(|c| finish_item_on(c, item.item_id, "done", Some(track_id), None))
            {
                Ok(()) => Outcome::Done(track_id),
                Err(e) => Outcome::Failed(Some(track_id), e.to_string()),
            };
        }

        let _ = self
            .lib
            .set_analysis_status(track_id, crate::types::AnalysisStatus::Running);

        self.set_stage(job, JobStage::Decode, path);
        let rate = self.cfg.analysis_rate;
        let decoded = match decode_file(path, rate, &job.cancel) {
            Ok(d) => d,
            Err(MediaError::Cancelled) => return Outcome::Interrupted(Some(track_id)),
            Err(e) => return Outcome::Failed(Some(track_id), e.to_string()),
        };
        let planar = decoded.view().to_planar();
        let sample_rate = decoded.sample_rate;
        drop(decoded);

        self.set_stage(job, JobStage::Analyze, path);
        let channels: [&[f32]; 2] = [&planar[0], &planar[1]];
        let view = AudioView {
            sample_rate,
            channels: &channels,
        };
        let mut result = AnalysisResult::default();
        for a in &active {
            match a.analyze(view, &mut result, &job.cancel, &mut |_| {}) {
                Ok(()) => {}
                Err(AnalysisError::Cancelled) => return Outcome::Interrupted(Some(track_id)),
                Err(e) => return Outcome::Failed(Some(track_id), e.to_string()),
            }
            if job.stopped() {
                return Outcome::Interrupted(Some(track_id));
            }
        }
        drop(planar);

        self.set_stage(job, JobStage::Persist, path);
        let stamp = AnalyzerStamp {
            analyzer: active.iter().map(|a| a.id()).collect::<Vec<_>>().join("+"),
            version: active.iter().map(|a| a.version()).max().unwrap_or(0),
            sample_rate: sample_rate.0,
        };
        let saved = self.lib.write_tx(|tx| {
            save_analysis_on(tx, track_id, &stamp, &result)?;
            finish_item_on(tx, item.item_id, "done", Some(track_id), None)
        });
        match saved {
            Ok(()) => Outcome::Done(track_id),
            Err(e) => Outcome::Failed(Some(track_id), e.to_string()),
        }
    }

    /// Dedupe by path, then size + fingerprint (full hash on collision). A new
    /// file gets a placeholder track row at once, under the dedupe lock.
    fn dedupe_and_reserve(
        &self,
        item: &WorkItem,
        path: &Path,
        meta: &fs::Metadata,
    ) -> Result<Reserved> {
        let path_str = normalize_path_string(path);
        let fp = fingerprint(path)?;
        let mut full: Option<[u8; 32]> = None;
        let _guard = lock(&self.dedupe_lock);

        if let Some(id) = self.lib.track_by_path(&path_str)? {
            // Same path: already imported. If it was flagged missing, it is back.
            let _ = self.lib.set_missing(id, false);
            return Ok(Reserved::Duplicate(id));
        }
        let candidates = self
            .lib
            .read(|c| tracks_by_fingerprint_on(c, fp.size, &fp.hash))?;
        for (id, existing) in candidates {
            let existing = Path::new(&existing);
            if !existing.exists() {
                // The old file is gone and an identical one appeared: a move. Relink.
                self.lib.relink_track(id, &path_str)?;
                return Ok(Reserved::Duplicate(id));
            }
            if fp.covers_whole_file() {
                return Ok(Reserved::Duplicate(id));
            }
            let mine = match full {
                Some(h) => h,
                None => *full.insert(full_hash(path)?),
            };
            if full_hash(existing)? == mine {
                return Ok(Reserved::Duplicate(id));
            }
        }

        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let new = NewTrack {
            path: path_str,
            file_size: fp.size,
            file_mtime: mtime_ms(meta),
            content_hash: Some(fp.hash.to_vec()),
            meta: TrackMeta {
                title: stem,
                ..TrackMeta::default()
            },
            tech: TrackTech {
                codec: path.extension().map(|e| e.to_string_lossy().to_lowercase()),
                ..TrackTech::default()
            },
            ..NewTrack::default()
        };
        let id = self.lib.write_tx(|tx| {
            let id = insert_track_on(tx, &new)?;
            tx.execute(
                "UPDATE import_job_items SET stage = 'dedupe', track_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![item.item_id, id.0, now_ms()],
            )?;
            Ok(id)
        })?;
        Ok(Reserved::New(id))
    }

    fn write_tags(&self, item: &WorkItem, track: TrackId, path: &Path) -> Result<()> {
        let tags: TrackTags = read_tags(path).unwrap_or_default();
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        self.lib.write_tx(|tx| {
            let artwork = match &tags.artwork {
                Some(a) if !a.data.is_empty() => Some(store_artwork_on(tx, &a.mime, &a.data)?),
                _ => None,
            };
            tx.execute(
                "UPDATE tracks SET title = ?2, artist = ?3, album = ?4, genre = ?5, year = ?6, \
                 comment = ?7, duration_secs = ?8, sample_rate = ?9, channels = ?10, \
                 bitrate_kbps = ?11, artwork_id = ?12 WHERE id = ?1",
                params![
                    track.0,
                    tags.title.or(stem),
                    tags.artist,
                    tags.album,
                    tags.genre,
                    tags.year,
                    tags.comment,
                    tags.duration.as_secs_f64(),
                    tags.sample_rate,
                    tags.channels,
                    tags.bitrate_kbps,
                    artwork
                ],
            )?;
            tx.execute(
                "UPDATE import_job_items SET stage = 'tags', updated_at = ?2 WHERE id = ?1",
                params![item.item_id, now_ms()],
            )?;
            Ok(())
        })
    }

    // ---- finalizers: playlists, target playlist, rekordbox data ----

    fn run_finalizer(&self, job: &Arc<JobRuntime>) -> Result<()> {
        self.lib.write_tx(|tx| {
            match job.kind {
                JobKind::ImportPlaylist => finalize_m3u(tx, job)?,
                JobKind::ImportRekordboxXml => finalize_rekordbox(tx, job)?,
                _ => {
                    if let Some(pl) = job.target_playlist {
                        let ids = item_tracks_in_order(tx, job.id)?;
                        add_tracks_on(tx, pl, &ids, None)?;
                    }
                }
            }
            set_job_state_on(tx, job.id, JobState::Completed, None)
        })
    }
}

enum Reserved {
    Duplicate(TrackId),
    New(TrackId),
}

fn path_item(p: &Path) -> NewItem {
    let norm = normalize_path(p);
    let path = norm.to_string_lossy().into_owned();
    let failed = if !norm.is_file() {
        Some("file not found".to_owned())
    } else if !discover::is_audio(&norm) {
        Some("unsupported format".to_owned())
    } else {
        None
    };
    NewItem {
        path,
        track_id: None,
        failed,
    }
}

/// File items for a files import (drag and drop): missing or unsupported ones become failed rows.
pub(crate) fn file_items(files: &[PathBuf]) -> Vec<NewItem> {
    files.iter().map(|p| path_item(p)).collect()
}

/// Items for a re-analysis job: the tracks' own paths with their ids.
pub(crate) fn reanalyze_items(lib: &Library, tracks: &[TrackId]) -> Result<Vec<NewItem>> {
    lib.read(|c| {
        let mut stmt = c.prepare_cached("SELECT path FROM tracks WHERE id = ?1")?;
        let mut out = Vec::with_capacity(tracks.len());
        for t in tracks {
            if let Some(path) = stmt
                .query_row([t.0], |r| r.get::<_, String>(0))
                .optional()?
            {
                out.push(NewItem {
                    path,
                    track_id: Some(*t),
                    failed: None,
                });
            }
        }
        Ok(out)
    })
}

fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

fn finish_item_on(
    c: &rusqlite::Connection,
    item_id: i64,
    status: &str,
    track: Option<TrackId>,
    error: Option<&str>,
) -> Result<()> {
    c.execute(
        "UPDATE import_job_items SET status = ?2, \
         stage = CASE WHEN ?2 = 'done' THEN 'persist' ELSE stage END, \
         track_id = COALESCE(?3, track_id), error = ?4, updated_at = ?5 WHERE id = ?1",
        params![item_id, status, track.map(|t| t.0), error, now_ms()],
    )?;
    Ok(())
}

/// Track ids of finished items in source order (duplicates included).
fn item_tracks_in_order(tx: &Transaction<'_>, job: JobId) -> Result<Vec<TrackId>> {
    let mut stmt = tx.prepare(
        "SELECT track_id FROM import_job_items \
         WHERE job_id = ?1 AND track_id IS NOT NULL AND status IN ('done', 'duplicate') \
         ORDER BY position, id",
    )?;
    let rows = stmt
        .query_map([job.0], |r| r.get(0).map(TrackId))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Normalized path to track id for the items of a job.
fn item_track_map(tx: &Transaction<'_>, job: JobId) -> Result<HashMap<String, TrackId>> {
    let mut stmt = tx.prepare(
        "SELECT path, track_id FROM import_job_items \
         WHERE job_id = ?1 AND track_id IS NOT NULL AND status IN ('done', 'duplicate')",
    )?;
    let rows = stmt
        .query_map([job.0], |r| {
            Ok((r.get::<_, String>(0)?, TrackId(r.get(1)?)))
        })?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    Ok(rows)
}

fn source_path(job: &JobRuntime) -> Result<PathBuf> {
    job.source
        .clone()
        .map(PathBuf::from)
        .ok_or_else(|| LibraryError::Corrupt("job has no source".into()))
}

fn finalize_m3u(tx: &Transaction<'_>, job: &JobRuntime) -> Result<()> {
    let file = source_path(job)?;
    let map = item_track_map(tx, job.id)?;
    // Re-read the file: the playlist keeps file order and repeated entries.
    let ids: Vec<TrackId> = discover::parse_m3u(&file)?
        .iter()
        .filter_map(|p| map.get(&normalize_path_string(p)).copied())
        .collect();
    let name = file.file_stem().map_or_else(
        || "Imported playlist".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let pl = create_playlist_on(tx, None, &name)?;
    add_tracks_on(tx, pl, &ids, None)?;
    Ok(())
}

fn finalize_rekordbox(tx: &Transaction<'_>, job: &JobRuntime) -> Result<()> {
    let file = source_path(job)?;
    let map = item_track_map(tx, job.id)?;
    let lib = rekordbox::parse(&fs::read_to_string(&file)?)?;
    let find = |loc: &Path| map.get(&normalize_path_string(loc)).copied();

    for t in &lib.tracks {
        let Some(id) = find(&t.location) else {
            continue;
        };
        // Fill what the file's own tags left empty (a title equal to the file name is the
        // import placeholder, not a tag); keep an existing rating.
        let stem = t
            .location
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned());
        tx.execute(
            "UPDATE tracks SET \
             title = CASE WHEN title IS NULL OR title = ?13 THEN COALESCE(?2, title) ELSE title END, \
             artist = COALESCE(artist, ?3), \
             album = COALESCE(album, ?4), genre = COALESCE(genre, ?5), \
             comment = COALESCE(comment, ?6), remixer = COALESCE(remixer, ?7), \
             label = COALESCE(label, ?8), year = COALESCE(year, ?9), \
             tag_bpm = COALESCE(tag_bpm, ?10), tag_key = COALESCE(tag_key, ?11), \
             rating = CASE WHEN rating = 0 THEN ?12 ELSE rating END WHERE id = ?1",
            params![
                id.0,
                t.title,
                t.artist,
                t.album,
                t.genre,
                t.comment,
                t.remixer,
                t.label,
                t.year,
                t.average_bpm,
                t.tonality,
                t.rating.unwrap_or(0),
                stem
            ],
        )?;
        // The grid was curated in rekordbox: keep it, locked, unless the user locked one here.
        if let Some(grid) = t.grid() {
            set_beat_grid_on(tx, id, &grid, true, true)?;
        }
        for cue in t.cues() {
            if !cue_exists(tx, id, &cue)? {
                add_cue_on(tx, id, &cue)?;
            }
        }
    }

    let by_id: HashMap<&str, &Path> = lib
        .tracks
        .iter()
        .map(|t| (t.track_id.as_str(), t.location.as_path()))
        .collect();
    let root_name = file.file_stem().map_or_else(
        || "rekordbox".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let root = create_folder_on(tx, None, &root_name)?;
    let resolve = |e: &RbEntry| match e {
        RbEntry::Id(k) => by_id.get(k.as_str()).and_then(|p| find(p)),
        RbEntry::Location(p) => find(p),
    };
    fn build(
        tx: &Transaction<'_>,
        parent: PlaylistId,
        nodes: &[RbNode],
        resolve: &dyn Fn(&RbEntry) -> Option<TrackId>,
    ) -> Result<()> {
        for n in nodes {
            match n {
                RbNode::Folder { name, children } => {
                    let f = create_folder_on(tx, Some(parent), name)?;
                    build(tx, f, children, resolve)?;
                }
                RbNode::Playlist { name, entries } => {
                    let p = create_playlist_on(tx, Some(parent), name)?;
                    let ids: Vec<TrackId> = entries.iter().filter_map(resolve).collect();
                    add_tracks_on(tx, p, &ids, None)?;
                }
            }
        }
        Ok(())
    }
    build(tx, root, &lib.playlists, &resolve)
}

/// A hot cue slot, or a cue of the same kind at the same position, already exists.
fn cue_exists(
    tx: &Transaction<'_>,
    track: TrackId,
    cue: &opendeck_contracts::cue::Cue,
) -> Result<bool> {
    use opendeck_contracts::cue::CueKind;
    let n: i64 = match cue.kind {
        CueKind::Hot { index } => tx.query_row(
            "SELECT count(*) FROM cues WHERE track_id = ?1 AND kind = 'hot' AND hot_index = ?2",
            params![track.0, index.index() as i64],
            |r| r.get(0),
        )?,
        CueKind::Memory | CueKind::Loop => tx.query_row(
            "SELECT count(*) FROM cues WHERE track_id = ?1 AND kind <> 'hot' AND abs(start_secs - ?2) < 0.001",
            params![track.0, cue.start.0],
            |r| r.get(0),
        )?,
    };
    Ok(n > 0)
}
