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
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::{Cue, HotCueIndex};
use opendeck_contracts::deck::DeckState;
use opendeck_contracts::mixer::{CrossfaderAssign, CrossfaderCurve, EqBand, MixerState};
use opendeck_contracts::settings::{AudioHostInfo, AudioHostKind, AudioSettings};
use opendeck_contracts::tempo::{SyncMode, TempoRange};
use opendeck_contracts::{Beats, DeckId, EngineCommand, Gain, MAX_DECKS, Seconds, pad::PadMode};
use opendeck_engine::device;

use crate::control::Msg;
pub use crate::control::{Backend, LaunchOptions, TICK};
pub use crate::settings_store::{SETTINGS_FILE, SettingsStore, StoredSettings};
pub use crate::types::{
    AudioDeviceInfo, AudioStatus, CoreError, CoreEvent, HotCues, OfflineAudio, TrackInfo,
    TrackLoadFailed, TrackLoaded,
};

/// How long a call waits for the control thread before giving up.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// Human-readable product name.
pub const PRODUCT_NAME: &str = "OpenDeck";

/// Crate version, shared by every workspace member.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn finite(value: f32, what: &str) -> Result<(), CoreError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!("{what} is not a finite number")))
    }
}

fn non_negative(value: f32, what: &str) -> Result<(), CoreError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "{what} must be a finite number, zero or more"
        )))
    }
}

fn positive(value: f64, what: &str) -> Result<(), CoreError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "{what} must be a finite number above zero"
        )))
    }
}

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

    // ----- sync, leader, quantize, slip, jog -----

    /// Sync mode of a deck.
    pub fn set_sync(&self, deck: DeckId, mode: SyncMode) -> Result<(), CoreError> {
        self.send(EngineCommand::SetSync { deck, mode })
    }

    /// Sync leader; `None` lets the engine choose.
    pub fn set_leader(&self, deck: Option<DeckId>) -> Result<(), CoreError> {
        self.send(EngineCommand::SetLeader { deck })
    }

    /// Quantize on or off.
    pub fn set_quantize(&self, deck: DeckId, on: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetQuantize { deck, on })
    }

    /// Slip mode on or off.
    pub fn set_slip(&self, deck: DeckId, on: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetSlip { deck, on })
    }

    /// Vinyl mode on or off.
    pub fn set_vinyl_mode(&self, deck: DeckId, on: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetVinylMode { deck, on })
    }

    /// Platter touched or released.
    pub fn jog_touch(&self, deck: DeckId, touched: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::JogTouch { deck, touched })
    }

    /// Platter rotation since the last call, in revolutions (signed).
    pub fn jog_turn(&self, deck: DeckId, revolutions: f32) -> Result<(), CoreError> {
        finite(revolutions, "revolutions")?;
        self.send(EngineCommand::JogTurn { deck, revolutions })
    }

    /// Fast search by `beats` (signed).
    pub fn jog_search(&self, deck: DeckId, beats: Beats) -> Result<(), CoreError> {
        finite(beats.0 as f32, "beats")?;
        self.send(EngineCommand::JogSearch { deck, beats })
    }

    // ----- loops, jumps, cues -----

    /// Loop in at the playhead.
    pub fn loop_in(&self, deck: DeckId) -> Result<(), CoreError> {
        self.send(EngineCommand::LoopIn { deck })
    }

    /// Loop out at the playhead and start looping.
    pub fn loop_out(&self, deck: DeckId) -> Result<(), CoreError> {
        self.send(EngineCommand::LoopOut { deck })
    }

    /// Exit the active loop or re-enter the last one.
    pub fn loop_toggle(&self, deck: DeckId) -> Result<(), CoreError> {
        self.send(EngineCommand::LoopToggle { deck })
    }

    /// Auto loop of `length` beats from the playhead.
    pub fn auto_loop(&self, deck: DeckId, length: Beats) -> Result<(), CoreError> {
        positive(length.0, "loop length")?;
        self.send(EngineCommand::AutoLoop { deck, length })
    }

    /// Loop roll of `length` beats while `active`.
    pub fn loop_roll(&self, deck: DeckId, length: Beats, active: bool) -> Result<(), CoreError> {
        positive(length.0, "roll length")?;
        self.send(EngineCommand::LoopRoll {
            deck,
            length,
            active,
        })
    }

    /// Multiplies the active loop length (`0.5` halves, `2.0` doubles).
    pub fn resize_loop(&self, deck: DeckId, factor: f32) -> Result<(), CoreError> {
        positive(f64::from(factor), "loop factor")?;
        self.send(EngineCommand::LoopResize { deck, factor })
    }

    /// Jumps by `beats` (signed, not zero).
    pub fn beat_jump(&self, deck: DeckId, beats: Beats) -> Result<(), CoreError> {
        if !beats.0.is_finite() || beats.0 == 0.0 {
            return Err(CoreError::Invalid(
                "beat jump must be a finite, non-zero number of beats".to_owned(),
            ));
        }
        self.send(EngineCommand::BeatJump { deck, beats })
    }

    /// Hot cue pad edge: jumps (or previews while held) on a set slot.
    pub fn trigger_hot_cue(
        &self,
        deck: DeckId,
        index: HotCueIndex,
        pressed: bool,
    ) -> Result<(), CoreError> {
        self.send(EngineCommand::HotCue {
            deck,
            index,
            pressed,
        })
    }

    /// Stores the playhead in a hot cue slot (replacing any cue there).
    pub fn set_hot_cue(&self, deck: DeckId, index: HotCueIndex) -> Result<Cue, CoreError> {
        self.ask(|reply| Msg::SetHotCue { deck, index, reply })?
    }

    /// Clears a hot cue slot.
    pub fn delete_hot_cue(&self, deck: DeckId, index: HotCueIndex) -> Result<(), CoreError> {
        self.post(Msg::DeleteHotCue { deck, index })
    }

    /// The deck's hot cue slots.
    pub fn hot_cues(&self, deck: DeckId) -> Result<HotCues, CoreError> {
        self.ask(|reply| Msg::GetHotCues(deck, reply))
    }

    /// Stores the playhead as a memory cue.
    pub fn add_memory_cue(&self, deck: DeckId) -> Result<Cue, CoreError> {
        self.ask(|reply| Msg::AddMemoryCue { deck, reply })?
    }

    /// Jumps to the next (`direction >= 0`) or previous memory cue.
    pub fn call_memory_cue(&self, deck: DeckId, direction: i8) -> Result<(), CoreError> {
        self.post(Msg::CallMemoryCue { deck, direction })
    }

    // ----- mixer -----

    /// Channel trim as linear gain.
    pub fn set_trim(&self, deck: DeckId, gain: Gain) -> Result<(), CoreError> {
        non_negative(gain.0, "trim")?;
        self.send(EngineCommand::SetTrim { deck, gain })
    }

    /// Channel EQ band as linear gain (`0` kills the band).
    pub fn set_eq(&self, deck: DeckId, band: EqBand, gain: Gain) -> Result<(), CoreError> {
        non_negative(gain.0, "EQ gain")?;
        self.send(EngineCommand::SetEq { deck, band, gain })
    }

    /// Channel filter in `[-1, 1]`.
    pub fn set_filter(&self, deck: DeckId, amount: f32) -> Result<(), CoreError> {
        finite(amount, "filter")?;
        self.send(EngineCommand::SetFilter { deck, amount })
    }

    /// Channel fader in `[0, 1]`.
    pub fn set_channel_fader(&self, deck: DeckId, position: f32) -> Result<(), CoreError> {
        finite(position, "fader")?;
        self.send(EngineCommand::SetChannelFader { deck, position })
    }

    /// Headphone cue for a channel.
    pub fn set_pfl(&self, deck: DeckId, on: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetPfl { deck, on })
    }

    /// Crossfader position in `[-1, 1]`.
    pub fn set_crossfader(&self, position: f32) -> Result<(), CoreError> {
        finite(position, "crossfader")?;
        self.send(EngineCommand::SetCrossfader { position })
    }

    /// Crossfader curve.
    pub fn set_crossfader_curve(&self, curve: CrossfaderCurve) -> Result<(), CoreError> {
        self.send(EngineCommand::SetCrossfaderCurve { curve })
    }

    /// Crossfader side of a channel.
    pub fn set_crossfader_assign(
        &self,
        deck: DeckId,
        assign: CrossfaderAssign,
    ) -> Result<(), CoreError> {
        self.send(EngineCommand::SetCrossfaderAssign { deck, assign })
    }

    /// Master output level as linear gain.
    pub fn set_master_level(&self, gain: Gain) -> Result<(), CoreError> {
        non_negative(gain.0, "master level")?;
        self.send(EngineCommand::SetMasterLevel { gain })
    }

    /// Headphone output level as linear gain.
    pub fn set_headphone_level(&self, gain: Gain) -> Result<(), CoreError> {
        non_negative(gain.0, "headphone level")?;
        self.send(EngineCommand::SetHeadphoneLevel { gain })
    }

    /// Headphone cue/master mix in `[0, 1]`.
    pub fn set_headphone_mix(&self, mix: f32) -> Result<(), CoreError> {
        finite(mix, "headphone mix")?;
        self.send(EngineCommand::SetHeadphoneMix { mix })
    }

    /// Split cue on or off.
    pub fn set_split_cue(&self, on: bool) -> Result<(), CoreError> {
        self.send(EngineCommand::SetSplitCue { on })
    }

    /// Mixer settings as the control thread last sent them to the engine.
    pub fn mixer_state(&self) -> Result<MixerState, CoreError> {
        self.ask(Msg::GetMixer)
    }

    /// Control state of a deck (the control thread's view; no pad mode).
    pub fn deck_state(&self, deck: DeckId) -> Result<DeckState, CoreError> {
        self.ask(|reply| Msg::GetDeck(deck, reply))
    }

    /// Unloads the deck and cancels a decode in flight.
    pub fn eject(&self, deck: DeckId) -> Result<(), CoreError> {
        self.post(Msg::Eject(deck))
    }

    /// Reads the tags of `path` on the calling thread, then queues a decode
    /// on the loader thread. Returns as soon as the tags are known; the deck
    /// is playable when [`CoreEvent::TrackLoaded`] arrives, and a decode
    /// failure arrives as [`CoreEvent::TrackLoadFailed`]. A newer load for the
    /// same deck cancels this one. The track has no beat grid until analysis
    /// (M3); see [`load_track_with_grid`](Self::load_track_with_grid).
    pub fn load_track(&self, deck: DeckId, path: &Path) -> Result<TrackInfo, CoreError> {
        self.load_track_with_grid(deck, path, None)
    }

    /// Like [`load_track`](Self::load_track), with the track's beat grid
    /// (seconds from the first frame). Sync, auto loop, roll and beat jump
    /// need one. Hot cues held for the deck are re-sent after the load.
    pub fn load_track_with_grid(
        &self,
        deck: DeckId,
        path: &Path,
        grid: Option<BeatGrid>,
    ) -> Result<TrackInfo, CoreError> {
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
            grid,
            reply,
        })??;
        Ok(info)
    }

    /// Sets or clears the beat grid of the track loaded in `deck` (analysis
    /// finished after the load, or a grid edit). Fails when no track is loaded.
    pub fn set_beat_grid(&self, deck: DeckId, grid: Option<BeatGrid>) -> Result<(), CoreError> {
        self.ask(|reply| Msg::SetBeatGrid { deck, grid, reply })?
    }

    /// Offline backend only: renders `frames` frames now, ahead of the wall
    /// clock, and returns the master and headphone audio. For headless tests
    /// and tools; queued commands apply at the first block.
    pub fn render_offline(&self, frames: usize) -> Result<OfflineAudio, CoreError> {
        self.ask(|reply| Msg::RenderOffline { frames, reply })?
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

    /// Control state of `deck`, with the pad mode this core owns.
    pub fn deck_state(&self, deck: DeckId) -> Result<DeckState, CoreError> {
        let mut state = self.audio()?.deck_state(deck)?;
        state.pad_mode = self.pad_mode(deck);
        Ok(state)
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
