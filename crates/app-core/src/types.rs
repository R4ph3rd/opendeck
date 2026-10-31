//! DTOs and errors shared between the control thread and the shell.

use std::path::Path;

use opendeck_contracts::cue::Cue;
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::settings::{ChannelPair, TalkOverSettings};
use opendeck_contracts::{DeckId, EngineEvent, EngineTelemetry, Frames, Gain};
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
    /// Library track, `None` for a file loaded straight from a path.
    pub id: Option<TrackId>,
    /// Analysed BPM, when the track is in the library and analysed.
    pub bpm: Option<f64>,
    /// Analysed key, Camelot notation (`8A`).
    pub key: Option<String>,
    /// Analysed key, musical notation (`Am`).
    pub key_musical: Option<String>,
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
            id: None,
            bpm: None,
            key: None,
            key_musical: None,
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
    /// Output latency in milliseconds: measured from backend callback
    /// timestamps when `latency_measured`, else the buffer size estimate.
    /// `None` when neither is known (device default buffer, no timestamps).
    pub latency_ms: Option<f32>,
    /// `latency_ms` comes from callback timestamps, not an estimate.
    #[serde(default)]
    pub latency_measured: bool,
    /// Master pair in use.
    pub master: Option<ChannelPair>,
    /// Master is played as the mono sum.
    #[serde(default)]
    pub master_mono: bool,
    /// Headphone pair in use.
    pub headphones: Option<ChannelPair>,
    /// Headphones are played as the mono sum.
    #[serde(default)]
    pub headphones_mono: bool,
    /// Booth pair in use.
    #[serde(default)]
    pub booth: Option<ChannelPair>,
    /// Booth is played as the mono sum.
    #[serde(default)]
    pub booth_mono: bool,
    /// The device could not carry the requested routing; buses were dropped
    /// or stereo is used.
    pub stereo_fallback: bool,
    /// Input device name when mic or aux is open.
    #[serde(default)]
    pub input_device: Option<String>,
    /// Input device channel count.
    #[serde(default)]
    pub input_channels: Option<u16>,
    /// Input latency in milliseconds (capture to engine), measured from
    /// callback timestamps. The mic path adds the input prefill and the
    /// output latency.
    #[serde(default)]
    pub input_latency_ms: Option<f32>,
    /// Frames the engine queues before it consumes input (mic latency).
    #[serde(default)]
    pub input_prefill_frames: Option<u32>,
    /// Why mic and aux are not available although routed.
    #[serde(default)]
    pub input_error: Option<String>,
    /// Buffer xruns the backend reported since the engine started (also in
    /// telemetry).
    #[serde(default)]
    pub xruns: u32,
    /// The device was lost and the core is waiting to reopen it.
    #[serde(default)]
    pub reconnecting: bool,
    /// Reopen attempts so far while `reconnecting`.
    #[serde(default)]
    pub reconnect_attempts: u32,
    /// Why the engine is not running, if it is not.
    pub error: Option<String>,
}

/// The eight hot cue slots of a deck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct HotCues {
    /// Deck the slots belong to.
    pub deck: DeckId,
    /// One entry per slot (`HOT_CUES_PER_DECK`), `None` when empty.
    pub slots: Vec<Option<Cue>>,
}

/// Booth, mic, aux and talk-over controls as the control thread last sent
/// them to the engine (and replays after a restart).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InputState {
    /// Booth output level, linear gain.
    pub booth_level: Gain,
    /// Microphone level, linear gain.
    pub mic_level: Gain,
    /// Microphone low shelf, linear gain (unity is flat).
    pub mic_eq_low: Gain,
    /// Microphone high shelf, linear gain (unity is flat).
    pub mic_eq_high: Gain,
    /// Microphone on.
    pub mic_on: bool,
    /// Aux input level, linear gain.
    pub aux_level: Gain,
    /// Aux channel fader in `[0, 1]`.
    pub aux_fader: f32,
    /// Talk-over (ducking).
    pub talk_over: TalkOverSettings,
}

/// Output that carries the test tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum TestToneOutput {
    /// Master pair.
    Master,
    /// Headphone pair.
    Headphones,
    /// Booth pair.
    Booth,
}

/// Audio rendered on demand by the offline backend
/// ([`AudioController::render_offline`](crate::AudioController::render_offline)).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OfflineAudio {
    /// Master bus, interleaved stereo.
    pub master: Vec<f32>,
    /// Headphone bus, interleaved stereo.
    pub headphones: Vec<f32>,
    /// Booth bus (master after the master level, times the booth level),
    /// interleaved stereo.
    pub booth: Vec<f32>,
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
    /// The stored cues of a library track changed (set, deleted).
    CuesChanged(TrackId),
    /// The stream was (re)started or stopped.
    AudioStatus(AudioStatus),
    /// The FX model changed (assignment, effects, parameters, pads).
    FxState(Box<crate::fx_types::FxStateDto>),
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
    /// An argument is out of range or not a finite number, or a row does not exist.
    #[error("invalid argument: {0}")]
    Invalid(String),
    /// The library database failed.
    #[error("library: {0}")]
    Library(String),
}
