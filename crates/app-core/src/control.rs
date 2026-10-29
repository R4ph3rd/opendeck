//! The control thread (ADR 0003): owns the audio output and the
//! [`EngineHandle`], turns calls into `EngineCommand`s, feeds decoded tracks
//! to the engine, polls telemetry at ~60 Hz and publishes events.
//!
//! Nothing here runs on the audio thread and nothing blocks on a ring.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError};
use opendeck_contracts::analysis::CancelFlag;
use opendeck_contracts::settings::AudioSettings;
use opendeck_contracts::tempo::TempoRange;
use opendeck_contracts::{
    DeckId, EngineCommand, EngineEvent, EngineTelemetry, MAX_DECKS, MVP_DECKS, SampleRate,
};
use opendeck_engine::device::{AudioOutput, open_output};
use opendeck_engine::{EngineConfig, EngineHandle, OfflineRenderer, TrackBuffer};

use crate::loader::{DecodeResult, LoadRequest};
use crate::settings_store::{SettingsStore, StoredSettings};
use crate::types::{AudioStatus, CoreError, CoreEvent, TrackInfo, TrackLoadFailed, TrackLoaded};

/// Control tick: telemetry is forwarded at this period (~60 Hz).
pub const TICK: Duration = Duration::from_millis(16);

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
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            backend: Backend::Device,
            store: SettingsStore::memory(),
            max_track_secs: 30.0 * 60.0,
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
        reply: Sender<Result<(), CoreError>>,
    },
    Eject(DeckId),
    Decoded(DecodeResult),
    SetSettings {
        settings: AudioSettings,
        reply: Sender<Result<AudioStatus, CoreError>>,
    },
    GetSettings(Sender<AudioSettings>),
    GetStatus(Sender<AudioStatus>),
    Subscribe(Sender<CoreEvent>),
    Shutdown,
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

/// Tempo state replayed after an engine restart.
#[derive(Debug, Clone, Copy)]
struct DeckControls {
    tempo_fader: f32,
    range: TempoRange,
    key_lock: bool,
}

impl Default for DeckControls {
    fn default() -> Self {
        Self {
            tempo_fader: 0.0,
            range: TempoRange::default(),
            key_lock: false,
        }
    }
}

/// A decoded track waiting for room on the hand-off ring.
struct Pending {
    deck: DeckId,
    buffer: TrackBuffer,
    info: TrackInfo,
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
    controls: [DeckControls; MAX_DECKS],
    pending: Vec<Pending>,
    latest: Option<EngineTelemetry>,
    last_sequence: Option<u64>,
}

/// Runs the control loop until [`Msg::Shutdown`] or every sender is gone.
pub(crate) fn run(opts: LaunchOptions, inbox: Receiver<Msg>, loader: Sender<LoadRequest>) {
    let settings = opts.store.load();
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
        controls: [DeckControls::default(); MAX_DECKS],
        pending: Vec::new(),
        latest: None,
        last_sequence: None,
    };
    let initial = control.settings.audio.clone();
    if let Err(err) = control.start_engine(&initial) {
        tracing::warn!(%err, "audio engine did not start");
        control.status = stopped_status(control.opts.backend, &err.to_string());
    }
    control.main_loop();
    control.cancel_all();
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
                reply,
            } => {
                let _ = reply.send(self.request_load(deck, path, info));
            }
            Msg::Eject(deck) => {
                self.invalidate(deck);
                self.loaded[deck.index()] = None;
                self.pending.retain(|p| p.deck != deck);
                self.command(EngineCommand::Eject { deck });
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
            _ => {}
        }
        if let Some(engine) = self.engine.as_mut()
            && engine.handle().send(command).is_err()
        {
            tracing::warn!(?command, "engine command ring full, command dropped");
        }
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
                self.pending.push(Pending { deck, buffer, info });
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
        while let Some(Pending { deck, buffer, info }) = queue.next() {
            let frames = buffer.duration();
            match engine.handle().load_track(deck, buffer, None) {
                Ok(()) => delivered.push((deck, info, frames)),
                Err(buffer) => {
                    self.pending.push(Pending { deck, buffer, info });
                    self.pending.extend(queue);
                    break;
                }
            }
        }
        for (deck, info, frames) in delivered {
            self.loaded[deck.index()] = Some(info.clone());
            self.publish(CoreEvent::TrackLoaded(TrackLoaded {
                deck,
                track: info,
                frames,
            }));
        }
    }

    // ----- tick -----

    fn tick(&mut self) {
        self.render_offline();
        self.flush_pending();
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
        for event in events {
            lost |= event == EngineEvent::DeviceLost;
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
        self.replay_state();
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
            if let Some(info) = self.loaded[deck.index()].clone() {
                let path = PathBuf::from(&info.path);
                if let Err(err) = self.request_load(*deck, path, info.clone()) {
                    tracing::warn!(%err, "could not reload track after restart");
                }
            }
        }
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
