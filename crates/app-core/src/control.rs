//! The control thread (ADR 0003): owns the audio output and the
//! [`EngineHandle`], turns calls into `EngineCommand`s, feeds decoded tracks
//! to the engine, polls telemetry at ~60 Hz and publishes events.
//!
//! Nothing here runs on the audio thread and nothing blocks on a ring.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use opendeck_contracts::analysis::CancelFlag;
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::{Cue, CueKind, FrameRegion, HotCueIndex, LoopRegion};
use opendeck_contracts::deck::{DeckState, PlayState};
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::mixer::{EqBand, MixerState};
use opendeck_contracts::settings::AudioSettings;
use opendeck_contracts::tempo::{SyncMode, TempoRange};
use opendeck_contracts::{
    DeckId, EngineCommand, EngineEvent, EngineTelemetry, HOT_CUES_PER_DECK, MAX_DECKS, MVP_DECKS,
    SampleRate, Seconds,
};
use opendeck_engine::device::{AudioOutput, open_output};
use opendeck_engine::{EngineConfig, EngineHandle, OfflineRenderer, TrackBuffer};
use opendeck_library::{Library, SavedSetEntry, now_ms};

use crate::fx::{FxController, FxEnv};
use crate::fx_types::{FxEffectDto, FxOp, FxPresetsDto, FxSlotRef, FxStateDto, FxUnitRef};
use crate::loader::{DecodeResult, LoadRequest};
use crate::settings_store::{SettingsStore, StoredSettings};
use crate::types::{
    AudioStatus, CoreError, CoreEvent, HotCues, OfflineAudio, TrackInfo, TrackLoadFailed,
    TrackLoaded,
};

/// Control tick: telemetry is forwarded at this period (~60 Hz).
pub const TICK: Duration = Duration::from_millis(16);

/// How far from the playhead a memory cue may be and still be deleted by
/// `delete_memory_cue`, in seconds.
const MEMORY_CUE_DELETE_REACH: f64 = 1.0;

/// Capacity of each subscriber queue. A stalled subscriber loses events
/// rather than growing memory.
const SUBSCRIBER_QUEUE: usize = 1024;

/// Where audio goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backend {
    /// A real output device through cpal.
    #[default]
    Device,
    /// No device: an [`OfflineRenderer`] driven at wall-clock pace. Used by
    /// headless integration tests. Audio settings are stored but have no effect.
    Offline {
        /// Engine sample rate in Hz.
        sample_rate: u32,
    },
}

/// How to launch the control thread.
#[derive(Debug, Clone)]
pub struct LaunchOptions {
    /// Audio backend.
    pub backend: Backend,
    /// Where settings persist.
    pub store: SettingsStore,
    /// Tracks longer than this are refused (ADR 0003 memory budget).
    pub max_track_secs: f64,
    /// Library that stores cues and the played-track history; `None` keeps
    /// cues in memory only (tests, tools).
    pub library: Option<Library>,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            backend: Backend::Device,
            store: SettingsStore::memory(),
            max_track_secs: 30.0 * 60.0,
            library: None,
        }
    }
}

/// Inbox of the control thread.
pub(crate) enum Msg {
    Command(EngineCommand),
    TogglePlaying(DeckId),
    SeekSeconds {
        deck: DeckId,
        seconds: f64,
    },
    Load {
        deck: DeckId,
        path: PathBuf,
        info: TrackInfo,
        grid: Option<BeatGrid>,
        reply: Sender<Result<(), CoreError>>,
    },
    Eject(DeckId),
    /// Replaces the beat grid of the loaded track.
    SetBeatGrid {
        deck: DeckId,
        grid: Option<BeatGrid>,
        reply: Sender<Result<(), CoreError>>,
    },
    /// Offline backend only: renders `frames` now and returns the audio.
    RenderOffline {
        frames: usize,
        reply: Sender<Result<OfflineAudio, CoreError>>,
    },
    /// Stores the playhead as a hot cue and tells the engine.
    SetHotCue {
        deck: DeckId,
        index: HotCueIndex,
        reply: Sender<Result<Cue, CoreError>>,
    },
    /// Clears a hot cue slot.
    DeleteHotCue {
        deck: DeckId,
        index: HotCueIndex,
    },
    /// Renames a hot cue.
    RenameHotCue {
        deck: DeckId,
        index: HotCueIndex,
        name: String,
    },
    /// Stores the playhead as a memory cue.
    AddMemoryCue {
        deck: DeckId,
        reply: Sender<Result<Cue, CoreError>>,
    },
    /// Jumps to the next (`>= 0`) or previous (`< 0`) memory cue.
    CallMemoryCue {
        deck: DeckId,
        direction: i8,
    },
    /// Deletes the memory cue nearest to the playhead (within a second).
    DeleteMemoryCue {
        deck: DeckId,
    },
    GetHotCues(DeckId, Sender<HotCues>),
    GetMixer(Sender<MixerState>),
    GetDeck(DeckId, Sender<DeckState>),
    Decoded(DecodeResult),
    SetSettings {
        settings: AudioSettings,
        reply: Sender<Result<AudioStatus, CoreError>>,
    },
    GetSettings(Sender<AudioSettings>),
    GetStatus(Sender<AudioStatus>),
    Subscribe(Sender<CoreEvent>),
    Fx(FxMsg),
    Shutdown,
}

/// FX calls handled by [`FxController`].
pub(crate) enum FxMsg {
    State(Sender<FxStateDto>),
    Effects(Sender<Vec<FxEffectDto>>),
    Presets(Sender<FxPresetsDto>),
    Apply(FxOp, Sender<Result<(), CoreError>>),
    Pad {
        deck: DeckId,
        index: usize,
        pressed: bool,
    },
    SavePreset {
        slot: FxSlotRef,
        name: String,
        reply: Sender<Result<FxPresetsDto, CoreError>>,
    },
    DeletePreset {
        effect: opendeck_contracts::fx::EffectKey,
        name: String,
        reply: Sender<Result<FxPresetsDto, CoreError>>,
    },
    SaveChain {
        unit: FxUnitRef,
        name: String,
        reply: Sender<Result<FxPresetsDto, CoreError>>,
    },
    DeleteChain {
        name: String,
        reply: Sender<Result<FxPresetsDto, CoreError>>,
    },
}

/// The engine half the control thread holds. One instance lives on the
/// control thread's stack, so the size difference between variants is moot.
#[allow(clippy::large_enum_variant)]
enum Engine {
    Device {
        output: AudioOutput,
        handle: EngineHandle,
    },
    Offline {
        renderer: OfflineRenderer,
        scratch: Vec<f32>,
        last_render: Instant,
    },
}

impl Engine {
    fn handle(&mut self) -> &mut EngineHandle {
        match self {
            Engine::Device { handle, .. } => handle,
            Engine::Offline { renderer, .. } => renderer.handle(),
        }
    }
}

/// Per-deck control state replayed after an engine restart and reported by
/// `get_deck_state`.
#[derive(Debug, Clone, Copy)]
struct DeckControls {
    tempo_fader: f32,
    range: TempoRange,
    key_lock: bool,
    key_shift: i8,
    sync: SyncMode,
    quantize: bool,
    slip: bool,
    vinyl: bool,
}

impl Default for DeckControls {
    fn default() -> Self {
        let empty = DeckState::empty(DeckId::ALL[0]);
        Self {
            tempo_fader: 0.0,
            range: TempoRange::default(),
            key_lock: false,
            key_shift: 0,
            sync: SyncMode::Off,
            quantize: empty.quantize,
            slip: false,
            vinyl: empty.vinyl_mode,
        }
    }
}

type HotCueSlots = [Option<Cue>; HOT_CUES_PER_DECK];

/// A decoded track waiting for room on the hand-off ring.
struct Pending {
    deck: DeckId,
    buffer: TrackBuffer,
    info: TrackInfo,
    grid: Option<BeatGrid>,
}

struct Control {
    opts: LaunchOptions,
    inbox: Receiver<Msg>,
    loader: Sender<LoadRequest>,
    settings: StoredSettings,
    engine: Option<Engine>,
    status: AudioStatus,
    subscribers: Vec<Sender<CoreEvent>>,
    generation: [u64; MAX_DECKS],
    cancel: [Option<CancelFlag>; MAX_DECKS],
    loaded: [Option<TrackInfo>; MAX_DECKS],
    /// Beat grid of the track in each deck (`None` until analysis, M3).
    grids: [Option<BeatGrid>; MAX_DECKS],
    /// Grid to send with the load in flight for each deck.
    requested_grids: [Option<BeatGrid>; MAX_DECKS],
    /// A grid change the hand-off ring had no room for; retried each tick.
    grid_unsent: [bool; MAX_DECKS],
    controls: [DeckControls; MAX_DECKS],
    mixer: MixerState,
    leader: Option<DeckId>,
    hot_cues: [HotCueSlots; MAX_DECKS],
    memory_cues: [Vec<Seconds>; MAX_DECKS],
    /// A reload after an engine restart: keep the cues instead of clearing them.
    restoring: [bool; MAX_DECKS],
    /// Saved set that collects the tracks played this session, made on the first load.
    current_set: Option<i64>,
    pending: Vec<Pending>,
    latest: Option<EngineTelemetry>,
    last_sequence: Option<u64>,
    fx: FxController,
}

/// Runs the control loop until [`Msg::Shutdown`] or every sender is gone.
pub(crate) fn run(opts: LaunchOptions, inbox: Receiver<Msg>, loader: Sender<LoadRequest>) {
    let settings = opts.store.load();
    let fx = FxController::new(opts.library.as_ref());
    let mut control = Control {
        opts,
        inbox,
        loader,
        settings,
        engine: None,
        status: AudioStatus::default(),
        subscribers: Vec::new(),
        generation: [0; MAX_DECKS],
        cancel: std::array::from_fn(|_| None),
        loaded: std::array::from_fn(|_| None),
        grids: [None; MAX_DECKS],
        requested_grids: [None; MAX_DECKS],
        grid_unsent: [false; MAX_DECKS],
        controls: [DeckControls::default(); MAX_DECKS],
        mixer: MixerState::default(),
        leader: None,
        hot_cues: std::array::from_fn(|_| std::array::from_fn(|_| None)),
        memory_cues: std::array::from_fn(|_| Vec::new()),
        restoring: [false; MAX_DECKS],
        current_set: None,
        pending: Vec::new(),
        latest: None,
        last_sequence: None,
        fx,
    };
    let initial = control.settings.audio.clone();
    if let Err(err) = control.start_engine(&initial) {
        tracing::warn!(%err, "audio engine did not start");
        control.status = stopped_status(control.opts.backend, &err.to_string());
    }
    control.main_loop();
    control.fx.flush(control.opts.library.as_ref());
    control.cancel_all();
}

/// Name of the saved set that collects a session, from its start time (UTC).
pub(crate) fn session_name(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "Session {year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn stopped_status(backend: Backend, reason: &str) -> AudioStatus {
    AudioStatus {
        backend: backend_name(backend).into(),
        error: Some(reason.to_owned()),
        ..AudioStatus::default()
    }
}

fn backend_name(backend: Backend) -> &'static str {
    match backend {
        Backend::Device => "device",
        Backend::Offline { .. } => "offline",
    }
}

impl Control {
    fn main_loop(&mut self) {
        let mut last_tick = Instant::now();
        loop {
            let wait = TICK.saturating_sub(last_tick.elapsed());
            match self.inbox.recv_timeout(wait) {
                Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(msg) => self.handle(msg),
                Err(RecvTimeoutError::Timeout) => {}
            }
            while let Ok(msg) = self.inbox.try_recv() {
                if matches!(msg, Msg::Shutdown) {
                    return;
                }
                self.handle(msg);
            }
            if last_tick.elapsed() >= TICK {
                last_tick = Instant::now();
                self.tick();
            }
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Command(command) => self.command(command),
            Msg::TogglePlaying(deck) => {
                let playing = self
                    .latest
                    .as_ref()
                    .is_some_and(|t| t.decks[deck.index()].playing);
                self.command(EngineCommand::SetPlaying {
                    deck,
                    playing: !playing,
                });
            }
            Msg::SeekSeconds { deck, seconds } => {
                if let Some(engine) = self.engine.as_mut() {
                    let rate = engine.handle().sample_rate();
                    let position = opendeck_contracts::Seconds(seconds.max(0.0)).to_frames(rate);
                    self.command(EngineCommand::Seek { deck, position });
                }
            }
            Msg::Load {
                deck,
                path,
                info,
                grid,
                reply,
            } => {
                self.restoring[deck.index()] = false;
                let _ = reply.send(self.request_load(deck, path, info, grid));
            }
            Msg::Eject(deck) => {
                self.invalidate(deck);
                self.loaded[deck.index()] = None;
                self.grids[deck.index()] = None;
                self.grid_unsent[deck.index()] = false;
                self.pending.retain(|p| p.deck != deck);
                self.command(EngineCommand::Eject { deck });
                self.clear_cues(deck);
            }
            Msg::SetBeatGrid { deck, grid, reply } => {
                let _ = reply.send(self.set_beat_grid(deck, grid));
            }
            Msg::RenderOffline { frames, reply } => {
                let _ = reply.send(self.render_now(frames));
            }
            Msg::SetHotCue { deck, index, reply } => {
                let _ = reply.send(self.set_hot_cue(deck, index));
            }
            Msg::DeleteHotCue { deck, index } => self.delete_hot_cue(deck, index),
            Msg::RenameHotCue { deck, index, name } => self.rename_hot_cue(deck, index, name),
            Msg::AddMemoryCue { deck, reply } => {
                let _ = reply.send(self.add_memory_cue(deck));
            }
            Msg::CallMemoryCue { deck, direction } => self.call_memory_cue(deck, direction),
            Msg::DeleteMemoryCue { deck } => self.delete_memory_cue(deck),
            Msg::GetHotCues(deck, reply) => {
                let _ = reply.send(self.hot_cue_snapshot(deck));
            }
            Msg::GetMixer(reply) => {
                let _ = reply.send(self.mixer);
            }
            Msg::GetDeck(deck, reply) => {
                let _ = reply.send(self.deck_state(deck));
            }
            Msg::Decoded(result) => self.decoded(result),
            Msg::SetSettings { settings, reply } => {
                let _ = reply.send(self.set_settings(settings));
            }
            Msg::GetSettings(reply) => {
                let _ = reply.send(self.settings.audio.clone());
            }
            Msg::GetStatus(reply) => {
                let _ = reply.send(self.status.clone());
            }
            Msg::Subscribe(tx) => self.subscribers.push(tx),
            Msg::Fx(msg) => self.fx_msg(msg),
            Msg::Shutdown => {}
        }
    }

    // ----- commands -----

    fn command(&mut self, command: EngineCommand) {
        match command {
            EngineCommand::SetTempoFader { deck, position } => {
                self.controls[deck.index()].tempo_fader = position;
            }
            EngineCommand::SetTempoRange { deck, range } => {
                self.controls[deck.index()].range = range;
            }
            EngineCommand::SetKeyLock { deck, on } => {
                self.controls[deck.index()].key_lock = on;
            }
            EngineCommand::SetKeyShift { deck, semitones } => {
                self.controls[deck.index()].key_shift = semitones.clamp(-12, 12);
            }
            EngineCommand::SetSync { deck, mode } => self.controls[deck.index()].sync = mode,
            EngineCommand::SetLeader { deck } => self.leader = deck,
            EngineCommand::SetQuantize { deck, on } => self.controls[deck.index()].quantize = on,
            EngineCommand::SetSlip { deck, on } => self.controls[deck.index()].slip = on,
            EngineCommand::SetVinylMode { deck, on } => self.controls[deck.index()].vinyl = on,
            other => self.track_mixer(other),
        }
        if let Some(engine) = self.engine.as_mut()
            && engine.handle().send(command).is_err()
        {
            tracing::warn!(?command, "engine command ring full, command dropped");
        }
    }

    /// Mirrors mixer commands into [`MixerState`] (clamped like the engine does).
    fn track_mixer(&mut self, command: EngineCommand) {
        let m = &mut self.mixer;
        match command {
            EngineCommand::SetTrim { deck, gain } => m.channels[deck.index()].trim = gain,
            EngineCommand::SetEq { deck, band, gain } => {
                m.channels[deck.index()].eq[band.index()] = gain;
            }
            EngineCommand::SetFilter { deck, amount } => {
                m.channels[deck.index()].filter = amount.clamp(-1.0, 1.0);
            }
            EngineCommand::SetChannelFader { deck, position } => {
                m.channels[deck.index()].fader = position.clamp(0.0, 1.0);
            }
            EngineCommand::SetPfl { deck, on } => m.channels[deck.index()].pfl = on,
            EngineCommand::SetCrossfaderAssign { deck, assign } => {
                m.channels[deck.index()].crossfader_assign = assign;
            }
            EngineCommand::SetCrossfader { position } => m.crossfader = position.clamp(-1.0, 1.0),
            EngineCommand::SetCrossfaderCurve { curve } => m.crossfader_curve = curve,
            EngineCommand::SetMasterLevel { gain } => m.master_level = gain,
            EngineCommand::SetHeadphoneLevel { gain } => m.headphone_level = gain,
            EngineCommand::SetHeadphoneMix { mix } => m.headphone_mix = mix.clamp(0.0, 1.0),
            EngineCommand::SetSplitCue { on } => m.split_cue = on,
            _ => {}
        }
    }

    /// Engine sample rate, from the newest snapshot or the open engine.
    fn rate(&mut self) -> Option<SampleRate> {
        if let Some(t) = &self.latest {
            return Some(t.sample_rate);
        }
        self.engine.as_mut().map(|e| e.handle().sample_rate())
    }

    /// Playhead of `deck` in seconds on the newest telemetry.
    fn playhead(&mut self, deck: DeckId) -> Result<Seconds, CoreError> {
        let rate = self
            .rate()
            .ok_or_else(|| CoreError::NoEngine("stopped".to_owned()))?;
        match &self.latest {
            Some(t) if t.decks[deck.index()].loaded => {
                Ok(t.decks[deck.index()].position.to_seconds(rate))
            }
            _ => Err(CoreError::Invalid("no track is loaded".to_owned())),
        }
    }

    // ----- cues (persisted in the library when one is open) -----

    /// Library track in `deck`, when cues can be stored.
    fn cue_target(&self, deck: DeckId) -> Option<(&Library, TrackId)> {
        let id = self.loaded[deck.index()].as_ref()?.id?;
        Some((self.opts.library.as_ref()?, id))
    }

    fn hot_cue_snapshot(&self, deck: DeckId) -> HotCues {
        HotCues {
            deck,
            slots: self.hot_cues[deck.index()].to_vec(),
        }
    }

    fn publish_hot_cues(&mut self, deck: DeckId) {
        let snapshot = self.hot_cue_snapshot(deck);
        self.publish(CoreEvent::HotCues(snapshot));
    }

    fn engine_hot_cue(&mut self, deck: DeckId, index: HotCueIndex, cue: Option<&Cue>) {
        let region = match (cue, self.rate()) {
            (Some(cue), Some(rate)) => Some(FrameRegion {
                start: cue.start.to_frames(rate),
                end: cue.end.map(|e| e.to_frames(rate)),
            }),
            (Some(_), None) => return,
            (None, _) => None,
        };
        self.command(EngineCommand::SetHotCue {
            deck,
            index,
            cue: region,
        });
    }

    /// Stores a hot cue in the library and returns it with its row id.
    fn persist_hot_cue(&self, deck: DeckId, mut cue: Cue) -> Cue {
        if let Some((lib, track)) = self.cue_target(deck) {
            match lib.add_cue(track, &cue) {
                Ok(id) => cue.id = Some(id),
                Err(err) => tracing::warn!(%err, "could not store hot cue"),
            }
        }
        cue
    }

    fn unpersist_hot_cue(&self, deck: DeckId, index: HotCueIndex, cue: &Cue) {
        let Some((lib, track)) = self.cue_target(deck) else {
            return;
        };
        let stored = match cue.id {
            Some(id) => Ok(Some(id)),
            None => lib.cues(track).map(|all| {
                all.into_iter()
                    .find(|c| matches!(c.kind, CueKind::Hot { index: i } if i == index))
                    .and_then(|c| c.id)
            }),
        };
        match stored {
            Ok(Some(id)) => {
                if let Err(err) = lib.delete_cue(id) {
                    tracing::warn!(%err, "could not delete hot cue");
                }
            }
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "could not find hot cue"),
        }
    }

    fn set_hot_cue(&mut self, deck: DeckId, index: HotCueIndex) -> Result<Cue, CoreError> {
        let start = self.playhead(deck)?;
        let cue = self.persist_hot_cue(
            deck,
            Cue {
                id: None,
                kind: CueKind::Hot { index },
                start,
                end: None,
                color: None,
                name: String::new(),
            },
        );
        self.hot_cues[deck.index()][index.index()] = Some(cue.clone());
        self.engine_hot_cue(deck, index, Some(&cue));
        self.publish_hot_cues(deck);
        self.cues_changed(deck);
        Ok(cue)
    }

    fn delete_hot_cue(&mut self, deck: DeckId, index: HotCueIndex) {
        if let Some(old) = self.hot_cues[deck.index()][index.index()].take() {
            self.unpersist_hot_cue(deck, index, &old);
            self.engine_hot_cue(deck, index, None);
            self.publish_hot_cues(deck);
            self.cues_changed(deck);
        }
    }

    fn rename_hot_cue(&mut self, deck: DeckId, index: HotCueIndex, name: String) {
        let Some(cue) = self.hot_cues[deck.index()][index.index()].as_mut() else {
            return;
        };
        cue.name = name;
        let cue = cue.clone();
        if let Some((lib, _)) = self.cue_target(deck)
            && cue.id.is_some()
            && let Err(err) = lib.update_cue(&cue)
        {
            tracing::warn!(%err, "could not rename hot cue");
        }
        self.publish_hot_cues(deck);
        self.cues_changed(deck);
    }

    /// Records a hot cue the engine set itself (pad press on an empty slot).
    fn engine_set_hot_cue(&mut self, deck: DeckId, index: HotCueIndex, region: FrameRegion) {
        let Some(rate) = self.rate() else {
            return;
        };
        let cue = self.persist_hot_cue(
            deck,
            Cue {
                id: None,
                kind: CueKind::Hot { index },
                start: region.start.to_seconds(rate),
                end: region.end.map(|e| e.to_seconds(rate)),
                color: None,
                name: String::new(),
            },
        );
        self.hot_cues[deck.index()][index.index()] = Some(cue);
        self.publish_hot_cues(deck);
        self.cues_changed(deck);
    }

    fn add_memory_cue(&mut self, deck: DeckId) -> Result<Cue, CoreError> {
        let start = self.playhead(deck)?;
        let fresh = !self.memory_cues[deck.index()].contains(&start);
        let mut cue = Cue {
            id: None,
            kind: CueKind::Memory,
            start,
            end: None,
            color: None,
            name: String::new(),
        };
        if fresh {
            if let Some((lib, track)) = self.cue_target(deck) {
                match lib.add_cue(track, &cue) {
                    Ok(id) => cue.id = Some(id),
                    Err(err) => tracing::warn!(%err, "could not store memory cue"),
                }
            }
            let cues = &mut self.memory_cues[deck.index()];
            cues.push(start);
            cues.sort_by(|a, b| a.0.total_cmp(&b.0));
            self.cues_changed(deck);
        }
        Ok(cue)
    }

    /// Removes the memory cue closest to the playhead, if one is within
    /// [`MEMORY_CUE_DELETE_REACH`] seconds, from memory and from the library.
    fn delete_memory_cue(&mut self, deck: DeckId) {
        let Ok(now) = self.playhead(deck) else {
            return;
        };
        let cues = &self.memory_cues[deck.index()];
        let Some((at, start)) = cues
            .iter()
            .copied()
            .enumerate()
            .map(|(i, c)| (i, c, (c.0 - now.0).abs()))
            .filter(|(_, _, d)| *d <= MEMORY_CUE_DELETE_REACH)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(i, c, _)| (i, c))
        else {
            return;
        };
        self.memory_cues[deck.index()].remove(at);
        if let Some((lib, track)) = self.cue_target(deck) {
            match lib.cues(track) {
                Ok(stored) => {
                    for cue in stored {
                        if cue.kind == CueKind::Memory
                            && (cue.start.0 - start.0).abs() < 1e-6
                            && let Some(id) = cue.id
                            && let Err(err) = lib.delete_cue(id)
                        {
                            tracing::warn!(%err, "could not delete memory cue");
                        }
                    }
                }
                Err(err) => tracing::warn!(%err, "could not read cues"),
            }
        }
        self.cues_changed(deck);
    }

    fn call_memory_cue(&mut self, deck: DeckId, direction: i8) {
        let Ok(now) = self.playhead(deck) else {
            return;
        };
        let Some(rate) = self.rate() else {
            return;
        };
        let cues = &self.memory_cues[deck.index()];
        // One frame of slack so a cue under the playhead is not "next".
        let slack = 1.0 / f64::from(rate.0);
        let target = if direction >= 0 {
            cues.iter().find(|c| c.0 > now.0 + slack)
        } else {
            cues.iter().rev().find(|c| c.0 < now.0 - slack)
        };
        if let Some(target) = target.copied() {
            let position = target.to_frames(rate);
            self.command(EngineCommand::SetCuePoint { deck, position });
            self.command(EngineCommand::Seek { deck, position });
        }
    }

    /// Forgets the cues of `deck` (new track or eject).
    fn clear_cues(&mut self, deck: DeckId) {
        self.memory_cues[deck.index()].clear();
        let had = self.hot_cues[deck.index()].iter().any(Option::is_some);
        self.hot_cues[deck.index()] = std::array::from_fn(|_| None);
        if had {
            self.publish_hot_cues(deck);
        }
    }

    /// Replaces the cues of `deck` with the stored ones of its library track.
    fn load_stored_cues(&mut self, deck: DeckId) {
        self.clear_cues(deck);
        let Some((lib, track)) = self.cue_target(deck) else {
            return;
        };
        let stored = match lib.cues(track) {
            Ok(stored) => stored,
            Err(err) => {
                tracing::warn!(%err, "could not read cues");
                return;
            }
        };
        let i = deck.index();
        for cue in stored {
            match cue.kind {
                CueKind::Hot { index } => self.hot_cues[i][index.index()] = Some(cue),
                CueKind::Memory => self.memory_cues[i].push(cue.start),
                CueKind::Loop => {}
            }
        }
        self.memory_cues[i].sort_by(|a, b| a.0.total_cmp(&b.0));
        if self.hot_cues[i].iter().any(Option::is_some) {
            self.publish_hot_cues(deck);
        }
    }

    /// Tells subscribers that the stored cues of the track in `deck` changed.
    fn cues_changed(&mut self, deck: DeckId) {
        let track = self.loaded[deck.index()].as_ref().and_then(|i| i.id);
        if let (Some(track), true) = (track, self.opts.library.is_some()) {
            self.publish(CoreEvent::CuesChanged(track));
        }
    }

    /// Counts a load as a play and appends the track to the session's saved set.
    fn record_played(&mut self, deck: DeckId, info: &TrackInfo) {
        let (Some(lib), Some(track)) = (self.opts.library.clone(), info.id) else {
            return;
        };
        if let Err(err) = lib.mark_played(track) {
            tracing::warn!(%err, "could not count the play");
        }
        let now = now_ms();
        let set = match self.current_set {
            Some(set) => set,
            None => match lib.create_saved_set(&session_name(now), "", None, Some(now), &[]) {
                Ok(set) => {
                    self.current_set = Some(set);
                    set
                }
                Err(err) => return tracing::warn!(%err, "could not start the session set"),
            },
        };
        let entry = SavedSetEntry {
            track: Some(track),
            title: info.title.clone(),
            artist: info.artist.clone(),
            deck: u8::try_from(deck.index()).ok(),
            played_at: Some(now),
        };
        if let Err(err) = lib.append_to_saved_set(set, &entry) {
            tracing::warn!(%err, "could not add the track to the session set");
        }
    }

    /// Sends the stored hot cues to a freshly loaded deck.
    fn resend_hot_cues(&mut self, deck: DeckId) {
        for i in 0..HOT_CUES_PER_DECK {
            let Some(index) = u8::try_from(i).ok().and_then(HotCueIndex::new) else {
                continue;
            };
            if let Some(cue) = self.hot_cues[deck.index()][i].clone() {
                self.engine_hot_cue(deck, index, Some(&cue));
            }
        }
    }

    fn deck_state(&self, deck: DeckId) -> DeckState {
        let c = &self.controls[deck.index()];
        let mut state = DeckState::empty(deck);
        state.tempo_range = c.range;
        state.tempo_fader = c.tempo_fader;
        state.key_lock = c.key_lock;
        state.key_shift = c.key_shift;
        state.sync = c.sync;
        state.quantize = c.quantize;
        state.slip = c.slip;
        state.vinyl_mode = c.vinyl;
        state.is_leader = self.leader == Some(deck);
        state.beat_grid = self.grids[deck.index()];
        state.track_bpm = state.beat_grid.map(|g| g.bpm());
        if let Some(t) = &self.latest {
            let d = &t.decks[deck.index()];
            state.play_state = match (d.loaded, d.playing) {
                (false, _) => PlayState::Empty,
                (true, true) => PlayState::Playing,
                (true, false) => PlayState::Paused,
            };
            state.is_leader = t.leader == Some(deck);
            state.active_loop = d.loop_region.filter(|_| d.loop_active).and_then(|r| {
                Some(LoopRegion {
                    start: r.start.to_seconds(t.sample_rate),
                    end: r.end?.to_seconds(t.sample_rate),
                })
            });
        }
        state
    }

    // ----- loading -----

    /// Cancels any decode in flight for `deck` and makes its result stale.
    fn invalidate(&mut self, deck: DeckId) -> u64 {
        let idx = deck.index();
        if let Some(flag) = self.cancel[idx].take() {
            flag.cancel();
        }
        self.generation[idx] += 1;
        self.generation[idx]
    }

    fn request_load(
        &mut self,
        deck: DeckId,
        path: PathBuf,
        info: TrackInfo,
        grid: Option<BeatGrid>,
    ) -> Result<(), CoreError> {
        let Some(engine) = self.engine.as_mut() else {
            return Err(CoreError::NoEngine(
                self.status
                    .error
                    .clone()
                    .unwrap_or_else(|| "stopped".to_owned()),
            ));
        };
        let rate = engine.handle().sample_rate();
        self.pending.retain(|p| p.deck != deck);
        let generation = self.invalidate(deck);
        self.requested_grids[deck.index()] = grid;
        let cancel = CancelFlag::new();
        self.cancel[deck.index()] = Some(cancel.clone());
        self.loader
            .send(LoadRequest {
                deck,
                generation,
                path,
                info,
                rate,
                cancel,
            })
            .map_err(|_| CoreError::NotRunning)
    }

    fn decoded(&mut self, result: DecodeResult) {
        let DecodeResult {
            deck,
            generation,
            info,
            outcome,
        } = result;
        if generation != self.generation[deck.index()] {
            return;
        }
        self.cancel[deck.index()] = None;
        let samples = match outcome {
            Ok(samples) => samples,
            Err(CoreError::Cancelled) => return,
            Err(err) => return self.load_failed(deck, &info, err.to_string()),
        };
        match TrackBuffer::from_vec(samples) {
            Ok(buffer) => {
                let grid = self.requested_grids[deck.index()];
                self.pending.push(Pending {
                    deck,
                    buffer,
                    info,
                    grid,
                });
                self.flush_pending();
            }
            Err(err) => self.load_failed(deck, &info, err.to_string()),
        }
    }

    fn load_failed(&mut self, deck: DeckId, info: &TrackInfo, message: String) {
        tracing::warn!(path = %info.path, %message, "track load failed");
        self.publish(CoreEvent::TrackLoadFailed(TrackLoadFailed {
            deck,
            path: info.path.clone(),
            message,
        }));
    }

    /// Hands pending buffers to the engine in order; keeps what does not fit.
    fn flush_pending(&mut self) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        let mut queue = std::mem::take(&mut self.pending).into_iter();
        let mut delivered = Vec::new();
        while let Some(Pending {
            deck,
            buffer,
            info,
            grid,
        }) = queue.next()
        {
            let frames = buffer.duration();
            match engine
                .handle()
                .load_track_with_grid(deck, buffer, None, grid)
            {
                Ok(()) => delivered.push((deck, info, frames, grid)),
                Err(buffer) => {
                    self.pending.push(Pending {
                        deck,
                        buffer,
                        info,
                        grid,
                    });
                    self.pending.extend(queue);
                    break;
                }
            }
        }
        for (deck, info, frames, grid) in delivered {
            self.grids[deck.index()] = grid;
            self.grid_unsent[deck.index()] = false;
            self.loaded[deck.index()] = Some(info.clone());
            // A new track takes its cues from the library; an engine restart
            // keeps the ones in memory.
            if !std::mem::take(&mut self.restoring[deck.index()]) {
                self.load_stored_cues(deck);
                self.record_played(deck, &info);
            }
            // The engine clears hot cues on every load.
            self.resend_hot_cues(deck);
            self.publish(CoreEvent::TrackLoaded(TrackLoaded {
                deck,
                track: info,
                frames,
            }));
        }
    }

    /// Sends a new grid for the loaded track; a full hand-off ring retries
    /// on the next tick.
    fn set_beat_grid(&mut self, deck: DeckId, grid: Option<BeatGrid>) -> Result<(), CoreError> {
        if self.loaded[deck.index()].is_none() {
            return Err(CoreError::Invalid("no track is loaded".to_owned()));
        }
        self.grids[deck.index()] = grid;
        self.grid_unsent[deck.index()] = true;
        self.flush_grids();
        Ok(())
    }

    fn flush_grids(&mut self) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        for deck in DeckId::ALL {
            let i = deck.index();
            if self.grid_unsent[i] && engine.handle().set_beat_grid(deck, self.grids[i]) {
                self.grid_unsent[i] = false;
            }
        }
    }

    // ----- tick -----

    fn tick(&mut self) {
        self.render_offline();
        self.flush_pending();
        self.flush_grids();
        let mut lost = false;
        let mut events: Vec<EngineEvent> = Vec::new();
        let mut telemetry = None;
        if let Some(engine) = self.engine.as_mut() {
            let handle = engine.handle();
            handle.reclaim();
            telemetry = handle.poll_telemetry();
            while let Some(event) = handle.poll_event() {
                events.push(event);
            }
            if let Engine::Device { output, .. } = engine {
                while let Some(event) = output.poll_event() {
                    events.push(event);
                }
            }
        }
        if let Some(snapshot) = telemetry {
            self.latest = Some(snapshot);
            if self.last_sequence != Some(snapshot.sequence) {
                self.last_sequence = Some(snapshot.sequence);
                self.publish(CoreEvent::Telemetry(Box::new(snapshot)));
            }
        }
        self.with_fx(|fx, env| fx.tick(env));
        for event in events {
            lost |= event == EngineEvent::DeviceLost;
            if let EngineEvent::HotCueSet { deck, index, cue } = event {
                self.engine_set_hot_cue(deck, index, cue);
            }
            self.publish(CoreEvent::Engine(event));
        }
        if lost {
            self.engine = None;
            self.pending.clear();
            self.status = stopped_status(self.opts.backend, "Output device lost");
            self.publish(CoreEvent::AudioStatus(self.status.clone()));
        }
    }

    /// In offline mode the "device" is this loop: render what wall-clock time
    /// says has elapsed, so playback runs at real speed without hardware.
    fn render_offline(&mut self) {
        let Some(Engine::Offline {
            renderer,
            scratch,
            last_render,
        }) = self.engine.as_mut()
        else {
            return;
        };
        let rate = u64::from(renderer.handle().sample_rate().0);
        let elapsed = last_render.elapsed();
        *last_render = Instant::now();
        let frames = ((elapsed.as_secs_f64() * rate as f64) as usize).min(rate as usize / 2);
        scratch.resize(frames * 2, 0.0);
        renderer.render_into(scratch);
    }

    /// Renders `frames` of master and headphones at once, ahead of the wall
    /// clock, so tests can inspect the audio. Offline backend only.
    fn render_now(&mut self, frames: usize) -> Result<OfflineAudio, CoreError> {
        self.flush_pending();
        self.flush_grids();
        let Some(Engine::Offline {
            renderer,
            last_render,
            ..
        }) = self.engine.as_mut()
        else {
            return Err(CoreError::Invalid(
                "rendering on demand needs the offline backend".to_owned(),
            ));
        };
        let mut audio = OfflineAudio {
            master: vec![0.0; frames * 2],
            headphones: vec![0.0; frames * 2],
        };
        renderer.render_both(&mut audio.master, &mut audio.headphones);
        // The wall clock resumes from here; what was just rendered is not repeated.
        *last_render = Instant::now();
        Ok(audio)
    }

    // ----- engine lifecycle -----

    fn start_engine(&mut self, settings: &AudioSettings) -> Result<(), CoreError> {
        let config = EngineConfig {
            deck_count: MVP_DECKS as u8,
            ..EngineConfig::default()
        };
        let (engine, status) = match self.opts.backend {
            Backend::Device => {
                let (output, handle) =
                    open_output(settings, config).map_err(|e| CoreError::Audio(e.to_string()))?;
                let info = output.info().clone();
                let status = AudioStatus {
                    running: true,
                    backend: "device".into(),
                    host: Some(info.host_id.clone()),
                    device: Some(info.device.clone()),
                    sample_rate: Some(info.sample_rate.0),
                    channels: Some(info.channels),
                    buffer_frames: info.buffer_frames,
                    latency_ms: info
                        .buffer_frames
                        .map(|f| f as f32 * 1000.0 / info.sample_rate.0 as f32),
                    master: Some(info.master),
                    headphones: info.headphones,
                    stereo_fallback: info.stereo_fallback,
                    error: None,
                };
                (Engine::Device { output, handle }, status)
            }
            Backend::Offline { sample_rate } => {
                let renderer = OfflineRenderer::new(EngineConfig {
                    sample_rate: SampleRate(sample_rate),
                    ..config
                })
                .map_err(|e| CoreError::Audio(e.to_string()))?;
                let status = AudioStatus {
                    running: true,
                    backend: "offline".into(),
                    host: Some("offline".into()),
                    device: Some("offline".into()),
                    sample_rate: Some(sample_rate),
                    channels: Some(2),
                    buffer_frames: None,
                    latency_ms: None,
                    master: Some(settings.master),
                    headphones: settings.headphones,
                    stereo_fallback: false,
                    error: None,
                };
                (
                    Engine::Offline {
                        renderer,
                        scratch: Vec::new(),
                        last_render: Instant::now(),
                    },
                    status,
                )
            }
        };
        self.engine = Some(engine);
        self.status = status;
        self.latest = None;
        self.last_sequence = None;
        self.pending.clear();
        self.replay_mixer();
        self.replay_state();
        self.with_fx(|fx, env| fx.resync(env));
        Ok(())
    }

    /// After a (re)start: restore tempo controls and reload loaded tracks at
    /// the new engine rate.
    fn replay_state(&mut self) {
        for deck in DeckId::ALL.iter().take(MVP_DECKS) {
            let controls = self.controls[deck.index()];
            self.command(EngineCommand::SetTempoRange {
                deck: *deck,
                range: controls.range,
            });
            self.command(EngineCommand::SetTempoFader {
                deck: *deck,
                position: controls.tempo_fader,
            });
            self.command(EngineCommand::SetKeyLock {
                deck: *deck,
                on: controls.key_lock,
            });
            self.command(EngineCommand::SetKeyShift {
                deck: *deck,
                semitones: controls.key_shift,
            });
            self.command(EngineCommand::SetSync {
                deck: *deck,
                mode: controls.sync,
            });
            self.command(EngineCommand::SetQuantize {
                deck: *deck,
                on: controls.quantize,
            });
            self.command(EngineCommand::SetSlip {
                deck: *deck,
                on: controls.slip,
            });
            self.command(EngineCommand::SetVinylMode {
                deck: *deck,
                on: controls.vinyl,
            });
            if let Some(info) = self.loaded[deck.index()].clone() {
                self.restoring[deck.index()] = true;
                let path = PathBuf::from(&info.path);
                let grid = self.grids[deck.index()];
                if let Err(err) = self.request_load(*deck, path, info.clone(), grid) {
                    tracing::warn!(%err, "could not reload track after restart");
                }
            }
        }
    }

    /// Re-sends the whole mixer after an engine restart.
    fn replay_mixer(&mut self) {
        let m = self.mixer;
        for ch in m.channels.iter().take(MVP_DECKS) {
            let deck = ch.deck;
            self.command(EngineCommand::SetTrim {
                deck,
                gain: ch.trim,
            });
            for band in [EqBand::Low, EqBand::Mid, EqBand::High] {
                self.command(EngineCommand::SetEq {
                    deck,
                    band,
                    gain: ch.eq[band.index()],
                });
            }
            self.command(EngineCommand::SetFilter {
                deck,
                amount: ch.filter,
            });
            self.command(EngineCommand::SetChannelFader {
                deck,
                position: ch.fader,
            });
            self.command(EngineCommand::SetPfl { deck, on: ch.pfl });
            self.command(EngineCommand::SetCrossfaderAssign {
                deck,
                assign: ch.crossfader_assign,
            });
        }
        self.command(EngineCommand::SetCrossfader {
            position: m.crossfader,
        });
        self.command(EngineCommand::SetCrossfaderCurve {
            curve: m.crossfader_curve,
        });
        self.command(EngineCommand::SetMasterLevel {
            gain: m.master_level,
        });
        self.command(EngineCommand::SetHeadphoneLevel {
            gain: m.headphone_level,
        });
        self.command(EngineCommand::SetHeadphoneMix {
            mix: m.headphone_mix,
        });
        self.command(EngineCommand::SetSplitCue { on: m.split_cue });
        self.command(EngineCommand::SetLeader { deck: self.leader });
    }

    fn set_settings(&mut self, settings: AudioSettings) -> Result<AudioStatus, CoreError> {
        let previous = self.settings.audio.clone();
        // Decodes in flight target the old rate.
        for deck in DeckId::ALL {
            self.invalidate(deck);
        }
        self.engine = None;
        match self.start_engine(&settings) {
            Ok(()) => {
                self.settings.audio = settings;
                if let Err(err) = self.opts.store.save(&self.settings) {
                    tracing::warn!(%err, "could not save settings");
                }
                self.publish(CoreEvent::AudioStatus(self.status.clone()));
                Ok(self.status.clone())
            }
            Err(err) => {
                // Best effort: go back to what worked.
                if self.start_engine(&previous).is_err() {
                    self.status = stopped_status(self.opts.backend, &err.to_string());
                }
                self.publish(CoreEvent::AudioStatus(self.status.clone()));
                Err(err)
            }
        }
    }

    // ----- FX -----

    /// Runs `f` with the FX controller and the control thread's view of the
    /// engine, then tells subscribers when the model changed.
    fn with_fx<R>(&mut self, f: impl FnOnce(&mut FxController, &mut FxEnv<'_>) -> R) -> R {
        let mut quantize = [false; MAX_DECKS];
        for (q, c) in quantize.iter_mut().zip(&self.controls) {
            *q = c.quantize;
        }
        let mut env = FxEnv {
            handle: self.engine.as_mut().map(Engine::handle),
            tel: self.latest.as_ref(),
            quantize,
            leader: self.latest.as_ref().and_then(|t| t.leader).or(self.leader),
            library: self.opts.library.as_ref(),
            now: Instant::now(),
        };
        let out = f(&mut self.fx, &mut env);
        if self.fx.take_dirty() {
            let state = Box::new(self.fx.state().clone());
            self.publish(CoreEvent::FxState(state));
        }
        out
    }

    fn fx_msg(&mut self, msg: FxMsg) {
        match msg {
            FxMsg::State(reply) => {
                let _ = reply.send(self.fx.state().clone());
            }
            FxMsg::Effects(reply) => {
                let _ = reply.send(self.fx.effects());
            }
            FxMsg::Presets(reply) => {
                let _ = reply.send(self.fx.presets());
            }
            FxMsg::Apply(op, reply) => {
                let result = self.with_fx(|fx, env| fx.apply(env, op));
                let _ = reply.send(result);
            }
            FxMsg::Pad {
                deck,
                index,
                pressed,
            } => {
                if let Err(err) = self.with_fx(|fx, env| fx.pad(env, deck, index, pressed)) {
                    tracing::warn!(%err, "pad FX ignored");
                }
            }
            FxMsg::SavePreset { slot, name, reply } => {
                let lib = self.opts.library.clone();
                let result = self
                    .fx
                    .slot_preset(slot, &name)
                    .and_then(|p| self.fx.save_preset(lib.as_ref(), p))
                    .map(|()| self.fx.presets());
                let _ = reply.send(result);
            }
            FxMsg::DeletePreset {
                effect,
                name,
                reply,
            } => {
                let lib = self.opts.library.clone();
                let result = self
                    .fx
                    .delete_preset(lib.as_ref(), &effect, &name)
                    .map(|()| self.fx.presets());
                let _ = reply.send(result);
            }
            FxMsg::SaveChain { unit, name, reply } => {
                let lib = self.opts.library.clone();
                let result = self
                    .fx
                    .save_chain(lib.as_ref(), unit, &name)
                    .map(|()| self.fx.presets());
                let _ = reply.send(result);
            }
            FxMsg::DeleteChain { name, reply } => {
                let lib = self.opts.library.clone();
                let result = self
                    .fx
                    .delete_chain(lib.as_ref(), &name)
                    .map(|()| self.fx.presets());
                let _ = reply.send(result);
            }
        }
    }

    // ----- publishing -----

    fn publish(&mut self, event: CoreEvent) {
        self.subscribers
            .retain(|tx| match tx.try_send(event.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) => true,
                Err(TrySendError::Disconnected(_)) => false,
            });
    }

    fn cancel_all(&mut self) {
        for flag in self.cancel.iter_mut().filter_map(Option::take) {
            flag.cancel();
        }
    }
}

/// Creates a bounded subscriber queue.
pub(crate) fn subscriber_channel() -> (Sender<CoreEvent>, Receiver<CoreEvent>) {
    crossbeam_channel::bounded(SUBSCRIBER_QUEUE)
}
