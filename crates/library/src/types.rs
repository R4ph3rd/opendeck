//! Plain data types returned and accepted by [`crate::Library`].

use opendeck_contracts::analysis::Loudness;
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::ids::{PlaylistId, TrackId};
use opendeck_contracts::key::MusicalKey;

/// Analysis state of a track (`tracks.analysis_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AnalysisStatus {
    /// Not analysed yet.
    #[default]
    Pending,
    /// A worker is analysing it.
    Running,
    /// Analysis stored.
    Done,
    /// Decoding or analysis failed.
    Failed,
    /// Analysed by an older analyzer version; re-analysis wanted.
    Stale,
}

impl AnalysisStatus {
    /// Column spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Stale => "stale",
        }
    }

    /// Parses the column spelling.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "stale" => Self::Stale,
            _ => return None,
        })
    }
}

/// Editable tag fields.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackMeta {
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
    /// BPM written in the tags (informational).
    pub tag_bpm: Option<f64>,
    /// Key written in the tags, as written.
    pub tag_key: Option<String>,
}

/// Technical properties of the file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackTech {
    /// Duration in seconds.
    pub duration_secs: Option<f64>,
    /// Sample rate of the file.
    pub sample_rate: Option<u32>,
    /// Channel count.
    pub channels: Option<u8>,
    /// Bitrate in kbit/s.
    pub bitrate_kbps: Option<u32>,
    /// Codec or container name.
    pub codec: Option<String>,
}

/// A track to insert.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NewTrack {
    /// Absolute, normalized path (see [`crate::normalize_path`]).
    pub path: String,
    /// File size in bytes.
    pub file_size: u64,
    /// File modification time, Unix ms.
    pub file_mtime: i64,
    /// Dedupe fingerprint (BLAKE3 of size + first and last 1 MiB).
    pub content_hash: Option<Vec<u8>>,
    /// Tags.
    pub meta: TrackMeta,
    /// File properties.
    pub tech: TrackTech,
    /// Artwork row, see [`crate::Library::store_artwork`].
    pub artwork_id: Option<i64>,
    /// Rating 0 to 5.
    pub rating: u8,
    /// Colour `0xRRGGBB`.
    pub color: Option<u32>,
    /// Unix ms; now when `None`.
    pub date_added: Option<i64>,
}

/// A full track row.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Row id.
    pub id: TrackId,
    /// Path.
    pub path: String,
    /// File size in bytes.
    pub file_size: u64,
    /// File modification time, Unix ms.
    pub file_mtime: i64,
    /// Dedupe fingerprint.
    pub content_hash: Option<Vec<u8>>,
    /// Tags.
    pub meta: TrackMeta,
    /// File properties.
    pub tech: TrackTech,
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
    pub analysis_status: AnalysisStatus,
    /// The file was not found on the last check.
    pub missing: bool,
}

/// Lightweight row for the collection view.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
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
    /// Duration in seconds.
    pub duration_secs: Option<f64>,
    /// Bitrate in kbit/s.
    pub bitrate_kbps: Option<u32>,
    /// Analysed BPM.
    pub bpm: Option<f64>,
    /// Analysed key, Camelot notation.
    pub key: Option<String>,
    /// Rating 0 to 5.
    pub rating: u8,
    /// Colour `0xRRGGBB`.
    pub color: Option<u32>,
    /// Unix ms.
    pub date_added: i64,
    /// Analysis state.
    pub analysis_status: AnalysisStatus,
    /// The file was not found on the last check.
    pub missing: bool,
    /// Artwork row.
    pub artwork_id: Option<i64>,
    /// Comment tag.
    pub comment: Option<String>,
}

/// Collection view sort column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortColumn {
    /// Title, case-insensitive.
    Title,
    /// Artist, case-insensitive.
    Artist,
    /// Album, case-insensitive.
    Album,
    /// Genre, case-insensitive.
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
    /// Analysed BPM.
    Bpm,
    /// Camelot key, by number then letter.
    Key,
    /// Rating.
    Rating,
    /// Analysis status.
    AnalysisStatus,
}

/// Collection view query: filter, sort and page.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackQuery {
    /// Free text over title, artist, album, genre and comment (every word must match as a prefix).
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
    pub status: Option<AnalysisStatus>,
    /// Include tracks whose file is missing.
    pub include_missing: bool,
    /// Sort column.
    pub sort: SortColumn,
    /// Sort descending.
    pub descending: bool,
    /// Rows to skip.
    pub offset: u32,
    /// Maximum rows.
    pub limit: u32,
}

impl Default for TrackQuery {
    fn default() -> Self {
        Self {
            text: None,
            bpm_min: None,
            bpm_max: None,
            keys: Vec::new(),
            genre: None,
            min_rating: None,
            status: None,
            include_missing: true,
            sort: SortColumn::default(),
            descending: false,
            offset: 0,
            limit: 100,
        }
    }
}

/// One page of a [`TrackQuery`].
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPage {
    /// Rows of this page.
    pub rows: Vec<TrackRow>,
    /// Rows matching the filter in total.
    pub total: u64,
}

/// Stored analysis of one track (without blobs for peaks).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredAnalysis {
    /// Analyzer id (ids joined with `+` when several ran).
    pub analyzer: String,
    /// Analyzer version (highest of those that ran).
    pub analyzer_version: u32,
    /// Unix ms.
    pub analyzed_at: i64,
    /// Rate of the analysed audio.
    pub sample_rate: u32,
    /// BPM.
    pub bpm: Option<f64>,
    /// Key.
    pub key: Option<MusicalKey>,
    /// Beat grid; `None` when absent or in an unknown blob version.
    pub beat_grid: Option<BeatGrid>,
    /// The user locked the grid; re-analysis keeps it.
    pub grid_locked: bool,
    /// Loudness.
    pub loudness: Option<Loudness>,
}

/// Who produced an analysis, stored with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerStamp {
    /// Analyzer id.
    pub analyzer: String,
    /// Analyzer version.
    pub version: u32,
    /// Rate of the analysed audio.
    pub sample_rate: u32,
}

/// Peak level description (no data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeakLevelInfo {
    /// Zoom level, 0 is finest.
    pub level: u32,
    /// Frames summarised by one point.
    pub frames_per_point: u32,
    /// Blob format version.
    pub format_version: u32,
    /// Blob size in bytes.
    pub byte_len: u32,
}

/// Playlist or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistKind {
    /// Holds playlists and folders, never tracks.
    Folder,
    /// Holds tracks.
    Playlist,
}

/// Node of the playlist tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistNode {
    /// Row id.
    pub id: PlaylistId,
    /// Parent folder, `None` at the top level.
    pub parent: Option<PlaylistId>,
    /// Folder or playlist.
    pub kind: PlaylistKind,
    /// Name.
    pub name: String,
    /// Index among siblings.
    pub position: u32,
    /// Tracks in the playlist (0 for folders).
    pub track_count: u32,
}

/// One entry of a playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistEntry {
    /// Entry id, stable across reorders; the same track may appear twice.
    pub entry_id: i64,
    /// Index in the playlist.
    pub position: u32,
    /// Track summary.
    pub track: TrackRow,
}

/// Saved set entry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SavedSetEntry {
    /// Track, `None` once it left the library.
    pub track: Option<TrackId>,
    /// Title snapshot.
    pub title: Option<String>,
    /// Artist snapshot.
    pub artist: Option<String>,
    /// Deck 0 to 3.
    pub deck: Option<u8>,
    /// Unix ms the track was played.
    pub played_at: Option<i64>,
}

/// Saved set header.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedSet {
    /// Row id.
    pub id: i64,
    /// Name.
    pub name: String,
    /// Notes.
    pub notes: String,
    /// JSON session snapshot.
    pub session_state: Option<String>,
    /// Unix ms the session started.
    pub started_at: Option<i64>,
    /// Unix ms.
    pub created_at: i64,
    /// Unix ms.
    pub updated_at: i64,
    /// Number of entries.
    pub track_count: u32,
}
