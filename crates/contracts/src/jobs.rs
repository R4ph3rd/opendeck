//! Import and analysis job progress, reported by the library job queue.
//!
//! A job processes items (files) through [`JobStage`]s. Job and item state
//! is persisted (`import_jobs`, `import_job_items`), so an interrupted job
//! resumes from its unfinished items after restart. Progress is emitted to
//! the UI as [`JobProgress`] events, throttled to a few per second per job.

use crate::ids::{JobId, TrackId};

/// What started the job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum JobKind {
    /// Recursive folder import.
    ImportFolder,
    /// Explicit list of files (drag and drop).
    ImportFiles,
    /// M3U/M3U8 playlist file.
    ImportPlaylist,
    /// rekordbox XML collection export.
    ImportRekordboxXml,
    /// Re-analysis of tracks already in the library.
    Reanalyze,
}

/// Pipeline stage of an item, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum JobStage {
    /// Walking folders or parsing the playlist/XML into items.
    Discover,
    /// Path and content-hash duplicate check.
    Dedupe,
    /// Tags and artwork.
    Tags,
    /// Decoding to PCM.
    Decode,
    /// BPM, grid, key, peaks, loudness.
    Analyze,
    /// Writing results to the database.
    Persist,
}

/// Lifecycle state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum JobState {
    /// Waiting for a worker.
    Queued,
    /// Processing items.
    Running,
    /// Paused by the user or interrupted by shutdown; resumable.
    Paused,
    /// Cancelled; finished items stay in the library.
    Cancelled,
    /// All items processed (some may have failed).
    Completed,
    /// The job itself failed (e.g. unreadable XML).
    Failed,
}

impl JobState {
    /// True for states that never change again.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            JobState::Cancelled | JobState::Completed | JobState::Failed
        )
    }
}

/// Outcome of one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum JobItemStatus {
    /// Not processed yet.
    Pending,
    /// Imported or analysed.
    Done,
    /// Already in the library (same path, or same size and content hash).
    Duplicate,
    /// Failed; the error text is stored with the item.
    Failed,
}

/// Progress snapshot of one job, sent to the UI.
///
/// Invariant: `items_done + items_failed + items_duplicate <= items_total`.
/// `items_total` may grow while `stage == Discover`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JobProgress {
    /// Job.
    pub job: JobId,
    /// Kind.
    pub kind: JobKind,
    /// State.
    pub state: JobState,
    /// Stage of the item being processed, if any.
    pub stage: Option<JobStage>,
    /// Items discovered so far.
    pub items_total: u32,
    /// Items done.
    pub items_done: u32,
    /// Items skipped as duplicates.
    pub items_duplicate: u32,
    /// Items failed.
    pub items_failed: u32,
    /// Path of the item being processed, for display.
    pub current_item: Option<String>,
    /// Track most recently added or updated, so the UI can refresh its row.
    pub last_track: Option<TrackId>,
}

impl JobProgress {
    /// Fraction of items finished in `[0, 1]`; 0 when nothing is known yet.
    pub fn fraction(&self) -> f32 {
        if self.items_total == 0 {
            return 0.0;
        }
        let finished = self.items_done + self.items_duplicate + self.items_failed;
        (finished as f32 / self.items_total as f32).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fraction() {
        let mut p = JobProgress {
            job: JobId(1),
            kind: JobKind::ImportFolder,
            state: JobState::Running,
            stage: Some(JobStage::Analyze),
            items_total: 0,
            items_done: 0,
            items_duplicate: 0,
            items_failed: 0,
            current_item: None,
            last_track: None,
        };
        assert_eq!(p.fraction(), 0.0);
        p.items_total = 4;
        p.items_done = 2;
        p.items_failed = 1;
        assert_eq!(p.fraction(), 0.75);
        assert!(!p.state.is_terminal());
        assert!(JobState::Cancelled.is_terminal());
        assert!(JobStage::Discover < JobStage::Persist);
    }
}
