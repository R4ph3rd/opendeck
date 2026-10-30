//! The library as the shell sees it: queries, edits, import jobs, playlists,
//! saved sets and loading a library track into a deck.
//!
//! [`LibraryService`] owns the [`Library`] handle and the [`ImportManager`].
//! A relay thread turns the import queue's progress into throttled
//! [`LibraryEvent`]s and, when analysis of a loaded track finishes, hands the
//! new beat grid to the deck. Every call is synchronous; call from a command
//! or worker thread, never from the audio thread.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TrySendError};
use opendeck_contracts::analysis::{AnalysisFeatures, Analyzer};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::Cue;
use opendeck_contracts::ids::{JobId, PlaylistId, TrackId};
use opendeck_contracts::jobs::{JobProgress, JobState};
use opendeck_contracts::{DeckId, MAX_DECKS};
use opendeck_library::import::{ImportConfig, ImportManager};
use opendeck_library::{Library, LibraryError, normalize_path_string};

use crate::AudioController;
use crate::dto::{
    ImportOptionsDto, JobInfo, LibraryChange, PlaylistEntryDto, PlaylistNodeDto, SavedSetDetailDto,
    SavedSetDto, SavedSetEntryDto, TrackAnalysisDto, TrackDetailDto, TrackMetaDto, TrackPageDto,
    TrackQueryDto, TrackRowDto,
};
use crate::types::{CoreError, TrackInfo};

/// Minimum time between `import-progress` events (about 10 Hz).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// How long a path load waits for a freshly imported track row to appear.
const PATH_IMPORT_WAIT: Duration = Duration::from_secs(3);

/// Jobs listed to the import panel (newest first).
const JOB_HISTORY: usize = 50;

/// Events the library publishes to subscribers.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryEvent {
    /// Progress of an import or analysis job, at most about 10 per second.
    ImportProgress(JobProgress),
    /// Tracks or playlists changed; coalesced to the same rate.
    LibraryChanged(LibraryChange),
    /// Analysis of a track was stored.
    AnalysisReady(TrackId),
    /// Cues of a track changed.
    CuesChanged(TrackId),
}

impl From<LibraryError> for CoreError {
    fn from(err: LibraryError) -> Self {
        match err {
            LibraryError::NotFound(_) | LibraryError::Invalid(_) => {
                CoreError::Invalid(err.to_string())
            }
            other => CoreError::Library(other.to_string()),
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the service last told a deck about its track.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LoadedDeck {
    id: TrackId,
    grid: Option<BeatGrid>,
}

struct Inner {
    lib: Library,
    imports: ImportManager,
    audio: Mutex<Option<AudioController>>,
    subscribers: Mutex<Vec<Sender<LibraryEvent>>>,
    loaded: Mutex<[Option<LoadedDeck>; MAX_DECKS]>,
}

impl Inner {
    fn publish(&self, event: &LibraryEvent) {
        lock(&self.subscribers).retain(|tx| match tx.try_send(event.clone()) {
            Ok(()) | Err(TrySendError::Full(_)) => true,
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    fn audio(&self) -> Result<AudioController, CoreError> {
        lock(&self.audio).clone().ok_or(CoreError::NotRunning)
    }

    /// Sends the stored grid of `track` to every deck that holds it, when it
    /// differs from what the deck has.
    fn sync_grids(&self, track: TrackId) {
        let Ok(audio) = self.audio() else {
            return;
        };
        let Ok(Some(analysis)) = self.lib.get_analysis(track) else {
            return;
        };
        let Some(grid) = analysis.beat_grid else {
            return;
        };
        let decks = *lock(&self.loaded);
        for (i, slot) in decks.iter().enumerate() {
            let Some(l) = slot else { continue };
            if l.id != track || l.grid == Some(grid) {
                continue;
            }
            let Some(deck) = u8::try_from(i).ok().and_then(DeckId::new) else {
                continue;
            };
            if audio.set_beat_grid(deck, Some(grid)).is_ok()
                && let Some(Some(current)) = lock(&self.loaded).get_mut(i)
                && current.id == track
            {
                current.grid = Some(grid);
            }
        }
    }

    fn track_touched(&self, track: TrackId) {
        if matches!(
            self.lib.analysis_status(track),
            Ok(Some(opendeck_library::AnalysisStatus::Done))
        ) {
            self.publish(&LibraryEvent::AnalysisReady(track));
            self.sync_grids(track);
        }
    }
}

/// Cloneable handle to the library. Dropping the last clone stops the import
/// workers; unfinished jobs stay resumable in the database.
#[derive(Clone)]
pub struct LibraryService {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for LibraryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryService")
            .field("library", &self.inner.lib)
            .finish_non_exhaustive()
    }
}

impl LibraryService {
    /// Opens the library file at `path`, starts the import workers with
    /// `analyzers` and resumes jobs left running by a crash or quit.
    pub fn open(
        path: &Path,
        analyzers: Vec<Arc<dyn Analyzer>>,
        config: ImportConfig,
    ) -> Result<Self, CoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| CoreError::Library(e.to_string()))?;
        }
        Ok(Self::from_library(Library::open(path)?, analyzers, config))
    }

    /// An in-memory library (tests, previews).
    pub fn open_in_memory(
        analyzers: Vec<Arc<dyn Analyzer>>,
        config: ImportConfig,
    ) -> Result<Self, CoreError> {
        Ok(Self::from_library(
            Library::open_in_memory()?,
            analyzers,
            config,
        ))
    }

    /// Wraps an open library. Calls `ImportManager::resume_pending` once.
    pub fn from_library(
        lib: Library,
        analyzers: Vec<Arc<dyn Analyzer>>,
        config: ImportConfig,
    ) -> Self {
        let (imports, progress) = ImportManager::new(lib.clone(), analyzers, config);
        let inner = Arc::new(Inner {
            lib,
            imports,
            audio: Mutex::new(None),
            subscribers: Mutex::new(Vec::new()),
            loaded: Mutex::new([None; MAX_DECKS]),
        });
        match inner.imports.resume_pending() {
            Ok(jobs) if !jobs.is_empty() => tracing::info!(count = jobs.len(), "resumed jobs"),
            Ok(_) => {}
            Err(err) => tracing::warn!(%err, "could not resume import jobs"),
        }
        let weak = Arc::downgrade(&inner);
        let spawned = std::thread::Builder::new()
            .name("opendeck-library-relay".into())
            .spawn(move || relay(weak, progress));
        if let Err(err) = spawned {
            tracing::warn!(%err, "library relay did not start");
        }
        Self { inner }
    }

    /// The default analyzers from `opendeck-analysis`, as the queue wants them.
    pub fn default_analyzers() -> Vec<Arc<dyn Analyzer>> {
        opendeck_analysis::default_analyzers()
            .into_iter()
            .map(Arc::from)
            .collect()
    }

    /// The underlying library handle (settings, cue persistence).
    pub fn library(&self) -> Library {
        self.inner.lib.clone()
    }

    /// Connects the audio engine, so loads and grid updates reach the decks.
    pub fn attach_audio(&self, audio: AudioController) {
        *lock(&self.inner.audio) = Some(audio);
    }

    /// Subscribes to progress and change events. The queue is bounded; a
    /// subscriber that stops reading loses events.
    pub fn subscribe(&self) -> Receiver<LibraryEvent> {
        let (tx, rx) = crossbeam_channel::bounded(256);
        lock(&self.inner.subscribers).push(tx);
        rx
    }

    fn changed(&self, tracks: bool, playlists: bool) {
        self.inner
            .publish(&LibraryEvent::LibraryChanged(LibraryChange {
                tracks,
                playlists,
            }));
    }

    // ----- tracks -----

    /// One page of the collection.
    pub fn query_tracks(&self, query: TrackQueryDto) -> Result<TrackPageDto, CoreError> {
        let page = self.inner.lib.query_tracks(&query.into())?;
        Ok(TrackPageDto {
            rows: page.rows.into_iter().map(TrackRowDto::from_row).collect(),
            total: page.total,
        })
    }

    /// Full row of a track.
    pub fn get_track(&self, track: TrackId) -> Result<TrackDetailDto, CoreError> {
        self.inner
            .lib
            .get_track(track)?
            .map(Into::into)
            .ok_or_else(|| CoreError::Invalid("track not found".into()))
    }

    /// Applies a tag patch.
    pub fn update_track_meta(&self, track: TrackId, patch: TrackMetaDto) -> Result<(), CoreError> {
        let current = self
            .inner
            .lib
            .get_track(track)?
            .ok_or_else(|| CoreError::Invalid("track not found".into()))?;
        self.inner
            .lib
            .update_track_meta(track, &patch.apply(current.meta))?;
        self.changed(true, false);
        Ok(())
    }

    /// Sets the star rating (0 to 5).
    pub fn set_rating(&self, track: TrackId, rating: u8) -> Result<(), CoreError> {
        Ok(self.inner.lib.set_rating(track, rating)?)
    }

    /// Removes tracks from the collection (files stay on disk).
    pub fn delete_tracks(&self, tracks: &[TrackId]) -> Result<u64, CoreError> {
        let n = self.inner.lib.delete_tracks(tracks)?;
        self.changed(true, true);
        Ok(n)
    }

    /// Artwork `(mime, bytes)` of a track, if it has any.
    pub fn artwork(&self, track: TrackId) -> Result<Option<(String, Vec<u8>)>, CoreError> {
        let Some(t) = self.inner.lib.get_track(track)? else {
            return Ok(None);
        };
        match t.artwork_id {
            Some(id) => Ok(self.inner.lib.artwork(id)?),
            None => Ok(None),
        }
    }

    /// Peak blob (`contracts::peaks` layout) of one zoom level, if analysed.
    pub fn peaks(&self, track: TrackId, level: u32) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(self.inner.lib.get_peaks(track, level)?)
    }

    /// Stored analysis of a track.
    pub fn get_analysis(&self, track: TrackId) -> Result<Option<TrackAnalysisDto>, CoreError> {
        Ok(self.inner.lib.get_analysis(track)?.map(Into::into))
    }

    /// Stores a hand-edited grid (locked against re-analysis) and sends it to
    /// the decks holding the track.
    pub fn set_beat_grid(&self, track: TrackId, grid: BeatGrid) -> Result<(), CoreError> {
        self.inner.lib.set_beat_grid(track, &grid, true)?;
        self.inner.sync_grids(track);
        self.inner.publish(&LibraryEvent::AnalysisReady(track));
        Ok(())
    }

    /// Stored cues of a track.
    pub fn list_cues(&self, track: TrackId) -> Result<Vec<Cue>, CoreError> {
        Ok(self.inner.lib.cues(track)?)
    }

    // ----- import jobs -----

    /// Imports a folder, recursively.
    pub fn import_folder(
        &self,
        path: &Path,
        options: ImportOptionsDto,
    ) -> Result<JobId, CoreError> {
        Ok(self.inner.imports.import_folder(path, options.into())?)
    }

    /// Imports files.
    pub fn import_files(
        &self,
        paths: &[PathBuf],
        options: ImportOptionsDto,
    ) -> Result<JobId, CoreError> {
        Ok(self.inner.imports.import_files(paths, options.into())?)
    }

    /// Imports an M3U/M3U8 playlist.
    pub fn import_playlist(
        &self,
        path: &Path,
        options: ImportOptionsDto,
    ) -> Result<JobId, CoreError> {
        Ok(self.inner.imports.import_playlist(path, options.into())?)
    }

    /// Imports a rekordbox XML export.
    pub fn import_rekordbox_xml(
        &self,
        path: &Path,
        options: ImportOptionsDto,
    ) -> Result<JobId, CoreError> {
        Ok(self
            .inner
            .imports
            .import_rekordbox_xml(path, options.into())?)
    }

    /// Re-analyses tracks; a locked grid survives.
    pub fn reanalyze(
        &self,
        tracks: &[TrackId],
        features: AnalysisFeatures,
    ) -> Result<JobId, CoreError> {
        Ok(self.inner.imports.reanalyze(tracks, features)?)
    }

    /// Cancels a job.
    pub fn cancel_job(&self, job: JobId) -> Result<(), CoreError> {
        Ok(self.inner.imports.cancel(job)?)
    }

    /// Pauses a running job.
    pub fn pause_job(&self, job: JobId) -> Result<(), CoreError> {
        Ok(self.inner.imports.pause(job)?)
    }

    /// Resumes a paused job.
    pub fn resume_job(&self, job: JobId) -> Result<(), CoreError> {
        Ok(self.inner.imports.resume(job)?)
    }

    /// Stored jobs, newest first.
    pub fn list_jobs(&self) -> Result<Vec<JobInfo>, CoreError> {
        let mut jobs: Vec<JobInfo> = self
            .inner
            .imports
            .jobs()?
            .into_iter()
            .map(JobInfo::from)
            .collect();
        jobs.truncate(JOB_HISTORY);
        Ok(jobs)
    }

    // ----- playlists -----

    /// The playlist tree, flat in sibling order.
    pub fn list_playlists(&self) -> Result<Vec<PlaylistNodeDto>, CoreError> {
        Ok(self
            .inner
            .lib
            .playlist_tree()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Creates a playlist or a folder.
    pub fn create_playlist(
        &self,
        parent: Option<PlaylistId>,
        name: &str,
        folder: bool,
    ) -> Result<PlaylistId, CoreError> {
        let id = if folder {
            self.inner.lib.create_folder(parent, name)?
        } else {
            self.inner.lib.create_playlist(parent, name)?
        };
        self.changed(false, true);
        Ok(id)
    }

    /// Renames a playlist or folder.
    pub fn rename_playlist(&self, playlist: PlaylistId, name: &str) -> Result<(), CoreError> {
        self.inner.lib.rename_playlist(playlist, name)?;
        self.changed(false, true);
        Ok(())
    }

    /// Deletes a playlist or folder with its subtree (tracks stay).
    pub fn delete_playlist(&self, playlist: PlaylistId) -> Result<(), CoreError> {
        self.inner.lib.delete_playlist(playlist)?;
        self.changed(false, true);
        Ok(())
    }

    /// Moves a node under `parent` at sibling index `position`.
    pub fn move_playlist(
        &self,
        playlist: PlaylistId,
        parent: Option<PlaylistId>,
        position: u32,
    ) -> Result<(), CoreError> {
        self.inner
            .lib
            .move_playlist(playlist, parent, position as usize)?;
        self.changed(false, true);
        Ok(())
    }

    /// Entries of a playlist in order.
    pub fn playlist_tracks(
        &self,
        playlist: PlaylistId,
    ) -> Result<Vec<PlaylistEntryDto>, CoreError> {
        let entries = self.inner.lib.playlist_entries(playlist)?;
        Ok(entries
            .into_iter()
            .map(|e| PlaylistEntryDto {
                entry_id: e.entry_id,
                position: e.position,
                track: TrackRowDto::from_row(e.track),
            })
            .collect())
    }

    /// Adds tracks at `position` (the end when `None`).
    pub fn add_to_playlist(
        &self,
        playlist: PlaylistId,
        tracks: &[TrackId],
        position: Option<u32>,
    ) -> Result<(), CoreError> {
        self.inner
            .lib
            .add_tracks_to_playlist(playlist, tracks, position.map(|p| p as usize))?;
        self.changed(false, true);
        Ok(())
    }

    /// Removes entries by entry id.
    pub fn remove_from_playlist(
        &self,
        playlist: PlaylistId,
        entries: &[i64],
    ) -> Result<(), CoreError> {
        self.inner.lib.remove_playlist_entries(playlist, entries)?;
        self.changed(false, true);
        Ok(())
    }

    /// Moves entries so the first ends at `to_index`.
    pub fn move_playlist_entries(
        &self,
        playlist: PlaylistId,
        entries: &[i64],
        to_index: u32,
    ) -> Result<(), CoreError> {
        self.inner
            .lib
            .move_playlist_entries(playlist, entries, to_index as usize)?;
        self.changed(false, true);
        Ok(())
    }

    // ----- saved sets -----

    /// Saved sets, newest first.
    pub fn list_saved_sets(&self) -> Result<Vec<SavedSetDto>, CoreError> {
        Ok(self
            .inner
            .lib
            .saved_sets()?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// One saved set with its entries.
    pub fn get_saved_set(&self, set: i64) -> Result<SavedSetDetailDto, CoreError> {
        let header = self
            .inner
            .lib
            .saved_sets()?
            .into_iter()
            .find(|s| s.id == set)
            .ok_or_else(|| CoreError::Invalid("saved set not found".into()))?;
        let entries = self
            .inner
            .lib
            .saved_set_entries(set)?
            .into_iter()
            .map(SavedSetEntryDto::from)
            .collect();
        Ok(SavedSetDetailDto {
            set: header.into(),
            entries,
        })
    }

    /// Renames a saved set.
    pub fn rename_saved_set(&self, set: i64, name: &str) -> Result<(), CoreError> {
        let header = self
            .inner
            .lib
            .saved_sets()?
            .into_iter()
            .find(|s| s.id == set)
            .ok_or_else(|| CoreError::Invalid("saved set not found".into()))?;
        self.inner.lib.update_saved_set(
            set,
            name,
            &header.notes,
            header.session_state.as_deref(),
        )?;
        Ok(())
    }

    /// Deletes a saved set.
    pub fn delete_saved_set(&self, set: i64) -> Result<(), CoreError> {
        self.inner.lib.delete_saved_set(set)?;
        Ok(())
    }

    // ----- decks -----

    /// Loads a library track into a deck with its stored beat grid, hot cues
    /// and memory cues. The deck is playable when `track-loaded` arrives.
    pub fn load_track(&self, deck: DeckId, id: TrackId) -> Result<TrackInfo, CoreError> {
        let audio = self.inner.audio()?;
        let lib = &self.inner.lib;
        let track = lib
            .get_track(id)?
            .ok_or_else(|| CoreError::Invalid("track not found".into()))?;
        let path = PathBuf::from(&track.path);
        if !path.is_file() {
            let _ = lib.set_missing(id, true);
            return Err(CoreError::Media(format!("file not found: {}", track.path)));
        }
        if track.missing {
            let _ = lib.set_missing(id, false);
        }
        let analysis = lib.get_analysis(id).ok().flatten();
        let grid = analysis.as_ref().and_then(|a| a.beat_grid);
        let info = TrackInfo {
            title: track.meta.title.clone(),
            artist: track.meta.artist.clone(),
            album: track.meta.album.clone(),
            duration_secs: track.tech.duration_secs.unwrap_or(0.0),
            id: Some(id),
            bpm: analysis.as_ref().and_then(|a| a.bpm),
            key: analysis.as_ref().and_then(|a| a.key).map(|k| k.camelot()),
            key_musical: analysis.as_ref().and_then(|a| a.key).map(|k| k.musical()),
            ..TrackInfo::bare(&path)
        };
        let idx = deck.index();
        let previous = lock(&self.inner.loaded)[idx].replace(LoadedDeck { id, grid });
        match audio.load_track_with_info(deck, &path, info, grid) {
            Ok(info) => {
                // Analysis may have finished between the read and the load.
                self.inner.sync_grids(id);
                Ok(info)
            }
            Err(err) => {
                lock(&self.inner.loaded)[idx] = previous;
                Err(err)
            }
        }
    }

    /// Loads a file that may not be in the library: imports it, then loads
    /// the new track by id. A file the importer does not take is loaded
    /// straight from the path, without analysis.
    pub fn load_track_path(&self, deck: DeckId, path: &Path) -> Result<TrackInfo, CoreError> {
        let audio = self.inner.audio()?;
        let key = normalize_path_string(path);
        let mut id = self.inner.lib.track_by_path(&key)?;
        if id.is_none() && path.is_file() {
            self.inner
                .imports
                .import_files(&[path.to_path_buf()], ImportOptionsDto::default().into())?;
            let deadline = Instant::now() + PATH_IMPORT_WAIT;
            while id.is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(40));
                id = self.inner.lib.track_by_path(&key)?;
            }
        }
        match id {
            Some(id) => self.load_track(deck, id),
            None => {
                lock(&self.inner.loaded)[deck.index()] = None;
                audio.load_track(deck, path)
            }
        }
    }

    /// Forgets which track a deck holds (eject).
    pub fn forget_deck(&self, deck: DeckId) {
        lock(&self.inner.loaded)[deck.index()] = None;
    }
}

/// Relays import progress at about 10 Hz and reacts to finished tracks.
fn relay(inner: Weak<Inner>, progress: Receiver<JobProgress>) {
    let mut pending: HashMap<JobId, JobProgress> = HashMap::new();
    let mut last_state: HashMap<JobId, JobState> = HashMap::new();
    let mut last_track: HashMap<JobId, TrackId> = HashMap::new();
    let mut touched: HashSet<TrackId> = HashSet::new();
    let (mut tracks_dirty, mut playlists_dirty) = (false, false);
    let mut last_flush = Instant::now();
    loop {
        let received = progress.recv_timeout(PROGRESS_INTERVAL);
        let disconnected = matches!(
            received,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected)
        );
        let Some(inner) = inner.upgrade() else {
            return;
        };
        if let Ok(p) = received {
            if let Some(t) = p.last_track
                && last_track.insert(p.job, t) != Some(t)
            {
                touched.insert(t);
                tracks_dirty = true;
            }
            let state_changed = last_state.insert(p.job, p.state) != Some(p.state);
            if state_changed || p.state.is_terminal() {
                pending.remove(&p.job);
                if p.state.is_terminal() {
                    tracks_dirty = true;
                    playlists_dirty = true;
                }
                inner.publish(&LibraryEvent::ImportProgress(p));
            } else {
                pending.insert(p.job, p);
            }
        }
        if disconnected || last_flush.elapsed() >= PROGRESS_INTERVAL {
            last_flush = Instant::now();
            for (_, p) in pending.drain() {
                inner.publish(&LibraryEvent::ImportProgress(p));
            }
            if tracks_dirty || playlists_dirty {
                inner.publish(&LibraryEvent::LibraryChanged(LibraryChange {
                    tracks: tracks_dirty,
                    playlists: playlists_dirty,
                }));
                tracks_dirty = false;
                playlists_dirty = false;
            }
            for t in touched.drain() {
                inner.track_touched(t);
            }
        }
        if disconnected {
            return;
        }
    }
}
