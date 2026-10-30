//! IPC DTOs for the library: plain serializable copies of the `opendeck-library`
//! types (which carry no serde or specta), so the shell never hand-writes a
//! duplicate. Conversions live next to the types.

use opendeck_contracts::analysis::{AnalysisFeatures, Loudness};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::ids::{PlaylistId, TrackId};
use opendeck_contracts::jobs::JobProgress;
use opendeck_contracts::key::MusicalKey;
use opendeck_library as lib;
use serde::{Deserialize, Serialize};

/// Analysis state of a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatusDto {
    /// Not analysed yet.
    Pending,
    /// A worker is analysing it.
    Running,
    /// Analysis stored.
    Done,
    /// Decoding or analysis failed.
    Failed,
    /// Analysed by an older analyzer version.
    Stale,
}

impl From<lib::AnalysisStatus> for AnalysisStatusDto {
    fn from(s: lib::AnalysisStatus) -> Self {
        match s {
            lib::AnalysisStatus::Pending => Self::Pending,
            lib::AnalysisStatus::Running => Self::Running,
            lib::AnalysisStatus::Done => Self::Done,
            lib::AnalysisStatus::Failed => Self::Failed,
            lib::AnalysisStatus::Stale => Self::Stale,
        }
    }
}

impl From<AnalysisStatusDto> for lib::AnalysisStatus {
    fn from(s: AnalysisStatusDto) -> Self {
        match s {
            AnalysisStatusDto::Pending => Self::Pending,
            AnalysisStatusDto::Running => Self::Running,
            AnalysisStatusDto::Done => Self::Done,
            AnalysisStatusDto::Failed => Self::Failed,
            AnalysisStatusDto::Stale => Self::Stale,
        }
    }
}

/// Collection sort column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SortColumnDto {
    /// Title.
    Title,
    /// Artist.
    Artist,
    /// Album.
    Album,
    /// Genre.
    Genre,
    /// Duration.
    Duration,
    /// Bitrate.
    Bitrate,
    /// Year.
    Year,
    /// Date added.
    #[default]
    DateAdded,
    /// BPM.
    Bpm,
    /// Camelot key.
    Key,
    /// Rating.
    Rating,
    /// Analysis status.
    AnalysisStatus,
}

impl From<SortColumnDto> for lib::SortColumn {
    fn from(c: SortColumnDto) -> Self {
        match c {
            SortColumnDto::Title => Self::Title,
            SortColumnDto::Artist => Self::Artist,
            SortColumnDto::Album => Self::Album,
            SortColumnDto::Genre => Self::Genre,
            SortColumnDto::Duration => Self::Duration,
            SortColumnDto::Bitrate => Self::Bitrate,
            SortColumnDto::Year => Self::Year,
            SortColumnDto::DateAdded => Self::DateAdded,
            SortColumnDto::Bpm => Self::Bpm,
            SortColumnDto::Key => Self::Key,
            SortColumnDto::Rating => Self::Rating,
            SortColumnDto::AnalysisStatus => Self::AnalysisStatus,
        }
    }
}

/// Collection query: filter, sort and page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackQueryDto {
    /// Free text over title, artist, album, genre and comment.
    pub text: Option<String>,
    /// Lowest BPM, inclusive.
    pub bpm_min: Option<f64>,
    /// Highest BPM, inclusive.
    pub bpm_max: Option<f64>,
    /// Camelot keys to include (any of); empty means all.
    pub keys: Vec<String>,
    /// Genre, case-insensitive exact match.
    pub genre: Option<String>,
    /// Minimum rating.
    pub min_rating: Option<u8>,
    /// Only this analysis status.
    pub status: Option<AnalysisStatusDto>,
    /// Sort column.
    pub sort: SortColumnDto,
    /// Sort descending.
    pub descending: bool,
    /// Rows to skip.
    pub offset: u32,
    /// Maximum rows.
    pub limit: u32,
}

impl From<TrackQueryDto> for lib::TrackQuery {
    fn from(q: TrackQueryDto) -> Self {
        Self {
            text: q.text,
            bpm_min: q.bpm_min,
            bpm_max: q.bpm_max,
            keys: q.keys,
            genre: q.genre,
            min_rating: q.min_rating,
            status: q.status.map(Into::into),
            include_missing: true,
            sort: q.sort.into(),
            descending: q.descending,
            offset: q.offset,
            limit: q.limit,
        }
    }
}

/// One collection row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackRowDto {
    /// Row id.
    pub id: TrackId,
    /// Path.
    pub path: String,
    /// Title.
    pub title: Option<String>,
    /// Artist.
    pub artist: Option<String>,
    /// Album.
    pub album: Option<String>,
    /// Genre.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Comment tag.
    pub comment: Option<String>,
    /// Duration in seconds.
    pub duration_secs: Option<f64>,
    /// Bitrate in kbit/s.
    pub bitrate_kbps: Option<u32>,
    /// Analysed BPM.
    pub bpm: Option<f64>,
    /// Analysed key, Camelot notation (`8A`).
    pub key: Option<String>,
    /// Analysed key, musical notation (`Am`).
    pub key_musical: Option<String>,
    /// Rating 0 to 5.
    pub rating: u8,
    /// Colour `0xRRGGBB`.
    pub color: Option<u32>,
    /// Unix ms the track was added.
    pub date_added: i64,
    /// Analysis state.
    pub analysis_status: AnalysisStatusDto,
    /// The file was not found on the last check.
    pub missing: bool,
    /// Artwork row, if the file has artwork.
    pub artwork_id: Option<i64>,
}

impl TrackRowDto {
    /// Converts a library row.
    pub fn from_row(r: lib::TrackRow) -> Self {
        let key_musical = r
            .key
            .as_deref()
            .and_then(|k| MusicalKey::parse(k).ok())
            .map(MusicalKey::musical);
        Self {
            id: r.id,
            path: r.path,
            title: r.title,
            artist: r.artist,
            album: r.album,
            genre: r.genre,
            year: r.year,
            comment: r.comment,
            duration_secs: r.duration_secs,
            bitrate_kbps: r.bitrate_kbps,
            bpm: r.bpm,
            key: r.key,
            key_musical,
            rating: r.rating,
            color: r.color,
            date_added: r.date_added,
            analysis_status: r.analysis_status.into(),
            missing: r.missing,
            artwork_id: r.artwork_id,
        }
    }
}

/// One page of a [`TrackQueryDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackPageDto {
    /// Rows of this page.
    pub rows: Vec<TrackRowDto>,
    /// Rows matching the filter in total.
    pub total: u64,
}

/// Editable tag fields. As a patch, `None` leaves a field as it is and an
/// empty string clears a text field.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackMetaDto {
    /// Title.
    pub title: Option<String>,
    /// Artist.
    pub artist: Option<String>,
    /// Album.
    pub album: Option<String>,
    /// Album artist.
    pub album_artist: Option<String>,
    /// Remixer.
    pub remixer: Option<String>,
    /// Label.
    pub label: Option<String>,
    /// Composer.
    pub composer: Option<String>,
    /// Genre.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Track number.
    pub track_number: Option<i32>,
    /// Comment.
    pub comment: Option<String>,
    /// BPM written in the tags.
    pub tag_bpm: Option<f64>,
    /// Key written in the tags.
    pub tag_key: Option<String>,
}

impl From<lib::TrackMeta> for TrackMetaDto {
    fn from(m: lib::TrackMeta) -> Self {
        Self {
            title: m.title,
            artist: m.artist,
            album: m.album,
            album_artist: m.album_artist,
            remixer: m.remixer,
            label: m.label,
            composer: m.composer,
            genre: m.genre,
            year: m.year,
            track_number: m.track_number,
            comment: m.comment,
            tag_bpm: m.tag_bpm,
            tag_key: m.tag_key,
        }
    }
}

impl TrackMetaDto {
    /// Applies this patch on top of `base`.
    pub fn apply(self, base: lib::TrackMeta) -> lib::TrackMeta {
        fn text(patch: Option<String>, old: Option<String>) -> Option<String> {
            match patch {
                None => old,
                Some(s) if s.trim().is_empty() => None,
                Some(s) => Some(s),
            }
        }
        lib::TrackMeta {
            title: text(self.title, base.title),
            artist: text(self.artist, base.artist),
            album: text(self.album, base.album),
            album_artist: text(self.album_artist, base.album_artist),
            remixer: text(self.remixer, base.remixer),
            label: text(self.label, base.label),
            composer: text(self.composer, base.composer),
            genre: text(self.genre, base.genre),
            year: self.year.or(base.year),
            track_number: self.track_number.or(base.track_number),
            comment: text(self.comment, base.comment),
            tag_bpm: self.tag_bpm.or(base.tag_bpm),
            tag_key: text(self.tag_key, base.tag_key),
        }
    }
}

/// A full track row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackDetailDto {
    /// Row id.
    pub id: TrackId,
    /// Path.
    pub path: String,
    /// File size in bytes.
    pub file_size: u64,
    /// Tags.
    pub meta: TrackMetaDto,
    /// Duration in seconds.
    pub duration_secs: Option<f64>,
    /// Sample rate of the file.
    pub sample_rate: Option<u32>,
    /// Channel count.
    pub channels: Option<u8>,
    /// Bitrate in kbit/s.
    pub bitrate_kbps: Option<u32>,
    /// Codec name.
    pub codec: Option<String>,
    /// Rating 0 to 5.
    pub rating: u8,
    /// Colour `0xRRGGBB`.
    pub color: Option<u32>,
    /// Times loaded and played.
    pub play_count: u32,
    /// Unix ms of the last play.
    pub last_played_at: Option<i64>,
    /// Artwork row.
    pub artwork_id: Option<i64>,
    /// Unix ms.
    pub date_added: i64,
    /// Analysis state.
    pub analysis_status: AnalysisStatusDto,
    /// The file was not found on the last check.
    pub missing: bool,
}

impl From<lib::Track> for TrackDetailDto {
    fn from(t: lib::Track) -> Self {
        Self {
            id: t.id,
            path: t.path,
            file_size: t.file_size,
            meta: t.meta.into(),
            duration_secs: t.tech.duration_secs,
            sample_rate: t.tech.sample_rate,
            channels: t.tech.channels,
            bitrate_kbps: t.tech.bitrate_kbps,
            codec: t.tech.codec,
            rating: t.rating,
            color: t.color,
            play_count: t.play_count,
            last_played_at: t.last_played_at,
            artwork_id: t.artwork_id,
            date_added: t.date_added,
            analysis_status: t.analysis_status.into(),
            missing: t.missing,
        }
    }
}

/// Stored analysis of a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackAnalysisDto {
    /// Analyzer ids joined with `+`.
    pub analyzer: String,
    /// Analyzer version.
    pub analyzer_version: u32,
    /// Unix ms.
    pub analyzed_at: i64,
    /// BPM.
    pub bpm: Option<f64>,
    /// Key, Camelot notation.
    pub key: Option<MusicalKey>,
    /// Key, musical notation.
    pub key_musical: Option<String>,
    /// Beat grid.
    pub beat_grid: Option<BeatGrid>,
    /// The grid survives re-analysis.
    pub grid_locked: bool,
    /// Loudness.
    pub loudness: Option<Loudness>,
}

impl From<lib::StoredAnalysis> for TrackAnalysisDto {
    fn from(a: lib::StoredAnalysis) -> Self {
        Self {
            analyzer: a.analyzer,
            analyzer_version: a.analyzer_version,
            analyzed_at: a.analyzed_at,
            bpm: a.bpm,
            key_musical: a.key.map(MusicalKey::musical),
            key: a.key,
            beat_grid: a.beat_grid,
            grid_locked: a.grid_locked,
            loudness: a.loudness,
        }
    }
}

/// Folder or playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum PlaylistKindDto {
    /// Holds playlists and folders.
    Folder,
    /// Holds tracks.
    Playlist,
}

/// Node of the playlist tree, flat with parent ids in sibling order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistNodeDto {
    /// Row id.
    pub id: PlaylistId,
    /// Parent folder, `None` at the top level.
    pub parent: Option<PlaylistId>,
    /// Folder or playlist.
    pub kind: PlaylistKindDto,
    /// Name.
    pub name: String,
    /// Index among siblings.
    pub position: u32,
    /// Tracks in the playlist (0 for folders).
    pub track_count: u32,
}

impl From<lib::PlaylistNode> for PlaylistNodeDto {
    fn from(n: lib::PlaylistNode) -> Self {
        Self {
            id: n.id,
            parent: n.parent,
            kind: match n.kind {
                lib::PlaylistKind::Folder => PlaylistKindDto::Folder,
                lib::PlaylistKind::Playlist => PlaylistKindDto::Playlist,
            },
            name: n.name,
            position: n.position,
            track_count: n.track_count,
        }
    }
}

/// One playlist entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlaylistEntryDto {
    /// Entry id, stable across reorders.
    pub entry_id: i64,
    /// Index in the playlist.
    pub position: u32,
    /// Track summary.
    pub track: TrackRowDto,
}

/// Saved set header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SavedSetDto {
    /// Row id.
    pub id: i64,
    /// Name.
    pub name: String,
    /// Notes.
    pub notes: String,
    /// Unix ms the session started.
    pub started_at: Option<i64>,
    /// Unix ms.
    pub created_at: i64,
    /// Unix ms.
    pub updated_at: i64,
    /// Number of entries.
    pub track_count: u32,
}

impl From<lib::SavedSet> for SavedSetDto {
    fn from(s: lib::SavedSet) -> Self {
        Self {
            id: s.id,
            name: s.name,
            notes: s.notes,
            started_at: s.started_at,
            created_at: s.created_at,
            updated_at: s.updated_at,
            track_count: s.track_count,
        }
    }
}

/// Saved set entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SavedSetEntryDto {
    /// Track, `None` once it left the library.
    pub track: Option<TrackId>,
    /// Title snapshot.
    pub title: Option<String>,
    /// Artist snapshot.
    pub artist: Option<String>,
    /// Deck index.
    pub deck: Option<u8>,
    /// Unix ms the track was played.
    pub played_at: Option<i64>,
}

impl From<lib::SavedSetEntry> for SavedSetEntryDto {
    fn from(e: lib::SavedSetEntry) -> Self {
        Self {
            track: e.track,
            title: e.title,
            artist: e.artist,
            deck: e.deck,
            played_at: e.played_at,
        }
    }
}

/// A saved set with its entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SavedSetDetailDto {
    /// Header.
    pub set: SavedSetDto,
    /// Entries in play order.
    pub entries: Vec<SavedSetEntryDto>,
}

/// Options of an import job.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ImportOptionsDto {
    /// Analysis to run.
    pub features: AnalysisFeatures,
    /// Playlist that receives the imported tracks.
    pub target_playlist: Option<PlaylistId>,
}

impl Default for ImportOptionsDto {
    fn default() -> Self {
        Self {
            features: AnalysisFeatures::ALL,
            target_playlist: None,
        }
    }
}

impl From<ImportOptionsDto> for lib::import::ImportOptions {
    fn from(o: ImportOptionsDto) -> Self {
        Self {
            features: o.features,
            target_playlist: o.target_playlist,
        }
    }
}

/// A stored job for the import panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct JobInfo {
    /// Counts and state, shaped like the `import-progress` event payload.
    pub progress: JobProgress,
    /// Folder, playlist or XML path.
    pub source: Option<String>,
    /// Failure message of a failed job.
    pub error: Option<String>,
    /// Unix ms the job was created.
    pub created_at: i64,
}

impl From<lib::import::JobSummary> for JobInfo {
    fn from(s: lib::import::JobSummary) -> Self {
        Self {
            progress: JobProgress {
                job: s.id,
                kind: s.kind,
                state: s.state,
                stage: None,
                items_total: s.items_total,
                items_done: s.items_done,
                items_duplicate: s.items_duplicate,
                items_failed: s.items_failed,
                current_item: None,
                last_track: None,
            },
            source: s.source,
            error: s.error,
            created_at: s.created_at,
        }
    }
}

/// What a `library-changed` event says was touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LibraryChange {
    /// Tracks were added, removed or edited.
    pub tracks: bool,
    /// Playlists, folders or their entries changed.
    pub playlists: bool,
}

/// Kind of a directory entry in the file explorer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FsEntryKind {
    /// A folder.
    Dir,
    /// A supported audio file.
    Audio,
    /// A playlist or collection file the importers read (`m3u`, `m3u8`, `xml`).
    Playlist,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FsEntry {
    /// File or folder name.
    pub name: String,
    /// Absolute path.
    pub path: String,
    /// Folder, audio file or playlist file.
    pub kind: FsEntryKind,
}

/// A directory listing for the file explorer pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DirListing {
    /// Listed folder; empty for the list of roots.
    pub path: String,
    /// Parent folder, `None` at a root.
    pub parent: Option<String>,
    /// Folders first, then files, each sorted by name.
    pub entries: Vec<FsEntry>,
}
