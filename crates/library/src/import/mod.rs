//! Import queue: folder, files, M3U/M3U8 and rekordbox XML imports, and
//! re-analysis, run as background jobs.
//!
//! ```text
//! discover -> dedupe -> tags + artwork -> decode -> analyze -> persist
//! ```
//!
//! [`ImportManager`] owns a worker pool (cores minus one by default). Starting
//! a job returns as soon as its row exists; discovery, hashing, decoding and
//! analysis run on worker threads, so neither the UI nor the audio thread
//! waits. Progress arrives as [`JobProgress`] on a channel, at most about four
//! events per second per job plus one on every state change. Cancel and pause
//! flip a [`CancelFlag`] that decode and the analyzers poll; workers return
//! within one packet or analyzer poll interval. Item rows record the resume
//! point, so [`ImportManager::resume_pending`] after a crash or quit continues
//! from the items that were not finished.

mod discover;
mod engine;
mod hash;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Receiver;
use opendeck_contracts::analysis::{AnalysisFeatures, Analyzer};
use opendeck_contracts::ids::{JobId, PlaylistId, TrackId};
use opendeck_contracts::jobs::{JobItemStatus, JobKind, JobProgress, JobStage, JobState};
use opendeck_contracts::units::SampleRate;

use crate::error::{LibraryError, Result};
use crate::library::Library;
use engine::{Engine, Source, Stop};

pub use hash::{Fingerprint, fingerprint};

/// Import queue settings.
#[derive(Debug, Clone)]
pub struct ImportConfig {
    /// Pipeline worker threads; 0 means available cores minus one (at least 1).
    pub workers: usize,
    /// Rate files are decoded to for analysis.
    pub analysis_rate: SampleRate,
    /// Minimum time between progress events of one job (state changes always send).
    pub progress_interval: Duration,
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            workers: 0,
            analysis_rate: SampleRate(44_100),
            progress_interval: Duration::from_millis(250),
        }
    }
}

/// Per-job options.
#[derive(Debug, Clone, Copy)]
pub struct ImportOptions {
    /// Analysis to run; analyzers that provide none of these are skipped.
    pub features: AnalysisFeatures,
    /// Playlist that receives the imported tracks (folder and files imports).
    pub target_playlist: Option<PlaylistId>,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            features: AnalysisFeatures::ALL,
            target_playlist: None,
        }
    }
}

/// A stored job, for the jobs list.
#[derive(Debug, Clone, PartialEq)]
pub struct JobSummary {
    /// Job id.
    pub id: JobId,
    /// What started it.
    pub kind: JobKind,
    /// Stored state.
    pub state: JobState,
    /// Folder, playlist or XML path.
    pub source: Option<String>,
    /// Failure message of a failed job.
    pub error: Option<String>,
    /// Unix ms.
    pub created_at: i64,
    /// Items stored.
    pub items_total: u32,
    /// Items imported.
    pub items_done: u32,
    /// Items skipped as duplicates.
    pub items_duplicate: u32,
    /// Items that failed (including files not found).
    pub items_failed: u32,
}

/// One item of a job, for the import report.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemReport {
    /// Normalized path.
    pub path: PathBuf,
    /// Result so far.
    pub status: JobItemStatus,
    /// Last stage completed.
    pub stage: Option<JobStage>,
    /// Track created or matched.
    pub track: Option<TrackId>,
    /// Failure reason, for example `file not found`.
    pub error: Option<String>,
}

/// The import queue. Dropping it stops the workers; unfinished jobs stay
/// resumable in the database.
pub struct ImportManager {
    engine: Engine,
}

impl ImportManager {
    /// Starts the worker pool. `analyzers` run in order on each decoded file;
    /// pass the real analyzers from `opendeck-analysis` or fakes in tests.
    /// Returns the manager and the progress receiver.
    pub fn new(
        library: Library,
        analyzers: Vec<Arc<dyn Analyzer>>,
        config: ImportConfig,
    ) -> (Self, Receiver<JobProgress>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let engine = Engine::start(library, analyzers, config, tx);
        (Self { engine }, rx)
    }

    fn start(&self, id: JobId, source: Source) -> Result<JobId> {
        self.engine.shared.launch(id, source)?;
        Ok(id)
    }

    /// Imports every supported file under `dir`, recursively.
    pub fn import_folder(&self, dir: &Path, options: ImportOptions) -> Result<JobId> {
        let source = crate::normalize_path_string(dir);
        let id = engine::create_job(
            &self.engine.shared.lib,
            JobKind::ImportFolder,
            Some(&source),
            options.features,
            options.target_playlist,
        )?;
        self.start(id, Source::Folder(PathBuf::from(source)))
    }

    /// Imports an explicit list of files (drag and drop).
    pub fn import_files(&self, files: &[PathBuf], options: ImportOptions) -> Result<JobId> {
        let id = engine::create_job(
            &self.engine.shared.lib,
            JobKind::ImportFiles,
            None,
            options.features,
            options.target_playlist,
        )?;
        self.start(id, Source::Files(engine::file_items(files)))
    }

    /// Imports the tracks of an M3U/M3U8 file and creates a playlist from it.
    pub fn import_playlist(&self, file: &Path, options: ImportOptions) -> Result<JobId> {
        let source = crate::normalize_path_string(file);
        let id = engine::create_job(
            &self.engine.shared.lib,
            JobKind::ImportPlaylist,
            Some(&source),
            options.features,
            None,
        )?;
        self.start(id, Source::Playlist(PathBuf::from(source)))
    }

    /// Imports a rekordbox XML export: tracks found on disk, their beat grids
    /// and cues, and the playlist tree. Tracks not found are listed in the
    /// job report as failed items with `file not found`.
    pub fn import_rekordbox_xml(&self, file: &Path, options: ImportOptions) -> Result<JobId> {
        let source = crate::normalize_path_string(file);
        let id = engine::create_job(
            &self.engine.shared.lib,
            JobKind::ImportRekordboxXml,
            Some(&source),
            options.features,
            None,
        )?;
        self.start(id, Source::Rekordbox(PathBuf::from(source)))
    }

    /// Re-analyses tracks already in the library. A locked beat grid survives.
    pub fn reanalyze(&self, tracks: &[TrackId], features: AnalysisFeatures) -> Result<JobId> {
        let lib = &self.engine.shared.lib;
        let items = engine::reanalyze_items(lib, tracks)?;
        let id = engine::create_job(lib, JobKind::Reanalyze, None, features, None)?;
        self.start(id, Source::Files(items))
    }

    /// Re-analyses one track.
    pub fn reanalyze_track(&self, track: TrackId, features: AnalysisFeatures) -> Result<JobId> {
        self.reanalyze(&[track], features)
    }

    /// Cancels a job. Workers stop within one decode packet or analyzer poll;
    /// unfinished items stay `pending` but the job is terminal and is not resumed.
    pub fn cancel(&self, job: JobId) -> Result<()> {
        match self.engine.shared.active(job) {
            Some(rt) => {
                rt.request_stop(Stop::Cancel);
                Ok(())
            }
            None => engine::cancel_stored(&self.engine.shared.lib, job),
        }
    }

    /// Pauses a running job; [`ImportManager::resume`] continues it, also after a restart.
    pub fn pause(&self, job: JobId) -> Result<()> {
        self.engine
            .shared
            .active(job)
            .ok_or(LibraryError::NotFound("running job"))?
            .request_stop(Stop::Pause);
        Ok(())
    }

    /// Continues a paused job (or one left running by a crash) from its unfinished items.
    pub fn resume(&self, job: JobId) -> Result<()> {
        let source = self.engine.shared.resume_source(job)?;
        self.start(job, source).map(|_| ())
    }

    /// Resumes every job left `queued` or `running` (crash or quit); call once at startup.
    /// Paused jobs stay paused. Returns the jobs resumed.
    pub fn resume_pending(&self) -> Result<Vec<JobId>> {
        let mut resumed = Vec::new();
        for id in self.engine.shared.resumable_ids()? {
            if self.engine.shared.active(id).is_none() {
                self.resume(id)?;
                resumed.push(id);
            }
        }
        Ok(resumed)
    }

    /// Jobs running in this process.
    pub fn active_jobs(&self) -> Vec<JobId> {
        self.engine.shared.active_ids()
    }

    /// Stored state of a job.
    pub fn job_state(&self, job: JobId) -> Result<JobState> {
        self.engine.shared.job_state(job)
    }

    /// All stored jobs, newest first.
    pub fn jobs(&self) -> Result<Vec<JobSummary>> {
        engine::list_jobs(&self.engine.shared.lib)
    }

    /// Items of a job in source order, including failures such as files not found.
    pub fn job_report(&self, job: JobId) -> Result<Vec<ItemReport>> {
        engine::job_report(&self.engine.shared.lib, job)
    }
}
