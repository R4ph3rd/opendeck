//! OpenDeck application core.
//!
//! Top of the crate graph (ADR 0002): wires the engine, library, analysis,
//! effects and MIDI together, runs the control thread and owns application
//! state (decks, pad modes, settings). The Tauri shell talks only to this
//! crate. See `CONTEXT/app-core.md`.
//!
//! # Threads (ADR 0003)
//!
//! [`AppCore::launch`] starts a control thread, which owns the audio output
//! and the engine handle, and a loader thread, which decodes files at the
//! engine rate. [`AudioController`] is the cloneable, `Sync` handle the shell
//! calls; every method either queues a message or waits for a short reply,
//! and none touches the audio thread.

mod control;
mod loader;
mod settings_store;
mod types;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::Sender;
use opendeck_contracts::settings::{AudioHostInfo, AudioHostKind, AudioSettings};
use opendeck_contracts::tempo::TempoRange;
use opendeck_contracts::{DeckId, EngineCommand, MAX_DECKS, Seconds, pad::PadMode};
use opendeck_engine::device;

use crate::control::Msg;
pub use crate::control::{Backend, LaunchOptions, TICK};
pub use crate::settings_store::{SETTINGS_FILE, SettingsStore, StoredSettings};
pub use crate::types::{
    AudioDeviceInfo, AudioStatus, CoreError, CoreEvent, TrackInfo, TrackLoadFailed, TrackLoaded,
};

/// How long a call waits for the control thread before giving up.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Human-readable product name.
pub const PRODUCT_NAME: &str = "OpenDeck";

/// Crate version, shared by every workspace member.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cloneable handle to the control thread. Dropping the last clone stops the
/// control and loader threads.
#[derive(Clone)]
pub struct AudioController {
    tx: Sender<Msg>,
    max_track_secs: f64,
    _guard: Arc<Guard>,
}

impl std::fmt::Debug for AudioController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioController").finish_non_exhaustive()
    }
}

struct Guard {
    tx: Sender<Msg>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
        if let Ok(threads) = self.threads.get_mut() {
            for thread in threads.drain(..) {
                let _ = thread.join();
            }
        }
    }
}

impl AudioController {
    /// Spawns the control and loader threads. Returns before the audio device
    /// is open; read the outcome with [`status`](Self::status) or the
    /// [`CoreEvent::AudioStatus`] event.
    pub fn launch(opts: LaunchOptions) -> std::io::Result<Self> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (load_tx, load_rx) = crossbeam_channel::unbounded();
        let max_track_secs = opts.max_track_secs;
        let loader = loader::spawn(load_rx, tx.clone())?;
        let control = std::thread::Builder::new()
            .name("opendeck-control".into())
            .spawn(move || control::run(opts, rx, load_tx))?;
        Ok(Self {
            tx: tx.clone(),
            max_track_secs,
            _guard: Arc::new(Guard {
                tx,
                threads: Mutex::new(vec![control, loader]),
            }),
        })
    }

    fn post(&self, msg: Msg) -> Result<(), CoreError> {
        self.tx.send(msg).map_err(|_| CoreError::NotRunning)
    }

    fn ask<T>(&self, build: impl FnOnce(Sender<T>) -> Msg) -> Result<T, CoreError> {
        let (reply, answer) = crossbeam_channel::bounded(1);
        self.post(build(reply))?;
        answer
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| CoreError::NotRunning)
    }

    /// Subscribes to telemetry, engine events, track loads and status changes.
    /// The queue is bounded; a subscriber that stops reading loses events.
    pub fn subscribe(&self) -> Result<crossbeam_channel::Receiver<CoreEvent>, CoreError> {
        let (tx, rx) = control::subscriber_channel();
        self.post(Msg::Subscribe(tx))?;
        Ok(rx)
    }

    /// Sends any engine command. Fire and forget: a full ring drops the
    /// command (logged), a stopped engine ignores it.
    pub fn send(&self, command: EngineCommand) -> Result<(), CoreError> {
        self.post(Msg::Command(command))
    }

    /// Play or pause.
    pub fn set_playing(&self, deck: DeckId, playing: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetPlaying { deck, playing })
    }

    /// Play when paused, pause when playing, judged on the newest telemetry.
    pub fn toggle_playing(&self, deck: DeckId) -> Result<(), CoreError> {
        self.post(Msg::TogglePlaying(deck))
    }

    /// CUE button edge.
    pub fn cue(&self, deck: DeckId, pressed: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::Cue { deck, pressed })
    }

    /// Moves the playhead to `position` seconds from the start.
    pub fn seek(&self, deck: DeckId, position: Seconds) -> Result<(), CoreError> {
        self.post(Msg::SeekSeconds {
            deck,
            seconds: position.0,
        })
    }

    /// Tempo fader position in `[-1, 1]`.
    pub fn set_tempo_fader(&self, deck: DeckId, position: f32) -> Result<(), CoreError> {
        self.send(EngineCommand::SetTempoFader { deck, position })
    }

    /// Tempo fader range.
    pub fn set_tempo_range(&self, deck: DeckId, range: TempoRange) -> Result<(), CoreError> {
        self.send(EngineCommand::SetTempoRange { deck, range })
    }

    /// Key lock on or off.
    pub fn set_key_lock(&self, deck: DeckId, on: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetKeyLock { deck, on })
    }

    /// Temporary pitch bend in `[-1, 1]`; `0` releases.
    pub fn pitch_bend(&self, deck: DeckId, amount: f32) -> Result<(), CoreError> {
        self.send(EngineCommand::PitchBend { deck, amount })
    }

    /// Unloads the deck and cancels a decode in flight.
    pub fn eject(&self, deck: DeckId) -> Result<(), CoreError> {
        self.post(Msg::Eject(deck))
    }

    /// Reads the tags of `path` on the calling thread, then queues a decode
    /// on the loader thread. Returns as soon as the tags are known; the deck
    /// is playable when [`CoreEvent::TrackLoaded`] arrives, and a decode
    /// failure arrives as [`CoreEvent::TrackLoadFailed`]. A newer load for the
    /// same deck cancels this one.
    pub fn load_track(&self, deck: DeckId, path: &Path) -> Result<TrackInfo, CoreError> {
        if let Err(source) = std::fs::metadata(path) {
            return Err(CoreError::Media(format!(
                "cannot read {}: {source}",
                path.display()
            )));
        }
        let info = match opendeck_media::read_tags(path) {
            Ok(tags) => TrackInfo::from_tags(path, &tags),
            // Unreadable tags do not stop a file that still decodes.
            Err(_) => TrackInfo::bare(path),
        };
        if info.duration_secs > self.max_track_secs {
            return Err(CoreError::TooLong {
                minutes: info.duration_secs / 60.0,
                limit_minutes: self.max_track_secs / 60.0,
            });
        }
        self.ask(|reply| Msg::Load {
            deck,
            path: path.to_path_buf(),
            info: info.clone(),
            reply,
        })??;
        Ok(info)
    }

    /// Current audio settings.
    pub fn audio_settings(&self) -> Result<AudioSettings, CoreError> {
        self.ask(Msg::GetSettings)
    }

    /// Stops the stream, applies `settings`, restarts, persists on success.
    /// Loaded tracks are reloaded at the new rate. On failure the previous
    /// settings are restored when possible.
    pub fn set_audio_settings(&self, settings: AudioSettings) -> Result<AudioStatus, CoreError> {
        self.ask(|reply| Msg::SetSettings { settings, reply })?
    }

    /// State of the audio output.
    pub fn status(&self) -> Result<AudioStatus, CoreError> {
        self.ask(Msg::GetStatus)
    }
}

/// Application state owned by the control side. Not touched by the audio
/// thread. The shell wraps it in a mutex; audio calls go through the
/// cloneable [`AudioController`] so the lock is never held while waiting.
#[derive(Debug, Clone)]
pub struct AppCore {
    pad_modes: [PadMode; MAX_DECKS],
    audio: Option<AudioController>,
}

impl Default for AppCore {
    fn default() -> Self {
        Self::new()
    }
}

impl AppCore {
    /// Fresh state without an audio engine: every deck in HOT CUE mode.
    pub fn new() -> Self {
        Self {
            pad_modes: [PadMode::HotCue; MAX_DECKS],
            audio: None,
        }
    }

    /// Fresh state with the control and loader threads running.
    pub fn launch(opts: LaunchOptions) -> std::io::Result<Self> {
        Ok(Self {
            audio: Some(AudioController::launch(opts)?),
            ..Self::new()
        })
    }

    /// The audio controller, if the core was launched with one.
    pub fn audio(&self) -> Result<AudioController, CoreError> {
        self.audio.clone().ok_or(CoreError::NotRunning)
    }

    /// Active pad mode of `deck`. Drives the pads and the middle UI panel.
    pub fn pad_mode(&self, deck: DeckId) -> PadMode {
        self.pad_modes[deck.index()]
    }

    /// Sets the pad mode of `deck` (from a MIDI mode button or a UI click).
    /// Returns true if it changed, so the caller emits a change event and LED
    /// feedback only when needed.
    pub fn set_pad_mode(&mut self, deck: DeckId, mode: PadMode) -> bool {
        let slot = &mut self.pad_modes[deck.index()];
        let changed = *slot != mode;
        *slot = mode;
        changed
    }

    /// Audio host APIs for the settings screen, from cpal. Hosts the settings
    /// enum does not model are left out. Falls back to a fixed list when the
    /// platform reports none.
    pub fn audio_hosts(&self) -> Vec<AudioHostInfo> {
        let mut hosts: Vec<AudioHostInfo> = device::list_hosts()
            .into_iter()
            .filter_map(|h| {
                Some(AudioHostInfo {
                    kind: h.kind?,
                    name: h.name,
                    available: true,
                    is_default: h.is_default,
                })
            })
            .collect();
        if hosts.is_empty() {
            return fallback_hosts();
        }
        if !hosts.iter().any(|h| h.is_default) {
            hosts[0].is_default = true;
        }
        hosts
    }

    /// Output devices of `host` (`None` for the platform default).
    pub fn audio_devices(
        &self,
        host: Option<AudioHostKind>,
    ) -> Result<Vec<AudioDeviceInfo>, CoreError> {
        let devices =
            device::list_output_devices(host).map_err(|e| CoreError::Audio(e.to_string()))?;
        Ok(devices
            .into_iter()
            .map(|d| AudioDeviceInfo {
                name: d.name,
                is_default: d.is_default,
                max_channels: d.max_channels,
                default_sample_rate: d.default_sample_rate,
                sample_rate_ranges: d.sample_rate_ranges.iter().map(|r| [r.0, r.1]).collect(),
                buffer_frames: d.buffer_frames.map(|r| [r.0, r.1]),
            })
            .collect())
    }
}

/// Hosts this platform build is expected to offer, used when cpal lists none.
fn fallback_hosts() -> Vec<AudioHostInfo> {
    let host = |kind, name: &str, available, is_default| AudioHostInfo {
        kind,
        name: name.to_owned(),
        available,
        is_default,
    };
    if cfg!(target_os = "windows") {
        vec![
            host(AudioHostKind::Wasapi, "WASAPI", true, true),
            host(AudioHostKind::Asio, "ASIO", false, false),
        ]
    } else if cfg!(target_os = "macos") {
        vec![host(AudioHostKind::CoreAudio, "Core Audio", true, true)]
    } else {
        vec![
            host(AudioHostKind::Alsa, "ALSA", true, true),
            host(AudioHostKind::Jack, "JACK", false, false),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_workspace() {
        // Kept in sync with the workspace by scripts/check-version-sync.mjs.
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "VERSION must be semver: {VERSION}");
        assert!(parts.iter().all(|p| p.parse::<u64>().is_ok()));
        assert_eq!(PRODUCT_NAME, "OpenDeck");
    }

    #[test]
    fn pad_mode_per_deck() {
        let mut core = AppCore::new();
        let deck2 = DeckId::new(1).expect("deck 2");
        assert_eq!(core.pad_mode(deck2), PadMode::HotCue);
        assert!(core.set_pad_mode(deck2, PadMode::Sampler));
        assert!(!core.set_pad_mode(deck2, PadMode::Sampler));
        assert_eq!(core.pad_mode(deck2), PadMode::Sampler);
        let deck1 = DeckId::new(0).expect("deck 1");
        assert_eq!(core.pad_mode(deck1), PadMode::HotCue);
    }

    #[test]
    fn exactly_one_default_host() {
        let hosts = AppCore::new().audio_hosts();
        assert_eq!(hosts.iter().filter(|h| h.is_default).count(), 1);
    }

    #[test]
    fn audio_calls_need_a_launched_core() {
        assert_eq!(AppCore::new().audio().unwrap_err(), CoreError::NotRunning);
    }
}
