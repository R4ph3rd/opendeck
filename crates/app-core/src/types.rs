//! DTOs and errors shared between the control thread and the shell.

use std::path::Path;

use opendeck_contracts::cue::Cue;
use opendeck_contracts::settings::ChannelPair;
use opendeck_contracts::{DeckId, EngineEvent, EngineTelemetry, Frames};
use opendeck_media::TrackTags;
use serde::{Deserialize, Serialize};

/// What the UI shows about a loaded file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackInfo {
    /// Absolute path of the audio file.
    pub path: String,
    /// File name without directories, shown when there is no title tag.
    pub file_name: String,
    /// Title tag.
    pub title: Option<String>,
    /// Artist tag.
    pub artist: Option<String>,
    /// Album tag.
    pub album: Option<String>,
    /// Length in seconds (from the decoded audio once loaded).
    pub duration_secs: f64,
}

impl TrackInfo {
    /// Info for a file whose tags could not be read.
    pub(crate) fn bare(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            title: None,
            artist: None,
            album: None,
            duration_secs: 0.0,
        }
    }

    pub(crate) fn from_tags(path: &Path, tags: &TrackTags) -> Self {
        Self {
            title: tags.title.clone(),
            artist: tags.artist.clone(),
            album: tags.album.clone(),
            duration_secs: tags.duration.as_secs_f64(),
            ..Self::bare(path)
        }
    }
}

/// A track reached the audio thread's hand-off ring and is playable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackLoaded {
    /// Deck the track went to.
    pub deck: DeckId,
    /// Track details.
    pub track: TrackInfo,
    /// Length in engine frames.
    pub frames: Frames,
}

/// A load failed (unreadable file, unsupported format, too long).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TrackLoadFailed {
    /// Deck the load was for.
    pub deck: DeckId,
    /// File that failed.
    pub path: String,
    /// Human-readable reason.
    pub message: String,
}

/// An output device and what it supports, for the settings screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AudioDeviceInfo {
    /// Name as the host reports it; used to select the device.
    pub name: String,
    /// The host's default output device.
    pub is_default: bool,
    /// Most output channels any configuration offers.
    pub max_channels: u16,
    /// Default sample rate in Hz.
    pub default_sample_rate: Option<u32>,
    /// Supported sample rate ranges `[min, max]` in Hz.
    pub sample_rate_ranges: Vec<[u32; 2]>,
    /// Supported buffer size range `[min, max]` in frames, when known.
    pub buffer_frames: Option<[u32; 2]>,
}

/// State of the audio output, for the top bar and the settings screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AudioStatus {
    /// A stream is open and the engine is rendering.
    pub running: bool,
    /// `"device"` or `"offline"` (no device, used by tests).
    pub backend: String,
    /// Host API name.
    pub host: Option<String>,
    /// Device name.
    pub device: Option<String>,
    /// Engine sample rate in Hz.
    pub sample_rate: Option<u32>,
    /// Device channel count.
    pub channels: Option<u16>,
    /// Requested buffer size in frames; `None` is the device default.
    pub buffer_frames: Option<u32>,
    /// Output buffer latency in milliseconds, known only for a fixed buffer.
    pub latency_ms: Option<f32>,
    /// Master pair in use.
    pub master: Option<ChannelPair>,
    /// Headphone pair in use.
    pub headphones: Option<ChannelPair>,
    /// The device could not carry the requested routing; stereo is used.
    pub stereo_fallback: bool,
    /// Why the engine is not running, if it is not.
    pub error: Option<String>,
}

/// The eight hot cue slots of a deck. Held in memory until persistence lands (M3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HotCues {
    /// Deck the slots belong to.
    pub deck: DeckId,
    /// One entry per slot (`HOT_CUES_PER_DECK`), `None` when empty.
    pub slots: Vec<Option<Cue>>,
}

/// Everything the control thread publishes to subscribers.
#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    /// Newest engine snapshot, at most every ~16 ms.
    Telemetry(Box<EngineTelemetry>),
    /// A discrete engine or device event (cue set, track ended, device lost).
    Engine(EngineEvent),
    /// A track is playable.
    TrackLoaded(TrackLoaded),
    /// A load failed.
    TrackLoadFailed(TrackLoadFailed),
    /// A deck's hot cue slots changed.
    HotCues(HotCues),
    /// The stream was (re)started or stopped.
    AudioStatus(AudioStatus),
}

/// Why a core call failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CoreError {
    /// The control thread is gone (shutdown).
    #[error("the audio control thread is not running")]
    NotRunning,
    /// No audio engine is open; the message says why.
    #[error("audio engine is not running: {0}")]
    NoEngine(String),
    /// Opening or listing devices failed.
    #[error("{0}")]
    Audio(String),
    /// Reading or decoding a file failed.
    #[error("{0}")]
    Media(String),
    /// The track exceeds the memory limit.
    #[error("track is {minutes:.0} minutes long; the limit is {limit_minutes:.0} minutes")]
    TooLong {
        /// Track length in minutes.
        minutes: f64,
        /// Configured limit in minutes.
        limit_minutes: f64,
    },
    /// The load was replaced by a newer one.
    #[error("load cancelled")]
    Cancelled,
    /// Reading or writing the settings file failed.
    #[error("settings: {0}")]
    Settings(String),
    /// An argument is out of range or not a finite number.
    #[error("invalid argument: {0}")]
    Invalid(String),
}
