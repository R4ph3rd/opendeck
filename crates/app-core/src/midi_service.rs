//! The MIDI service: one thread that owns the controller worker, the bridge
//! and the user's mapping overrides (milestone 6, ADR 0007).
//!
//! ```text
//! Controller (midi crate)            MidiService thread                  app-core
//!   events() ------ControlEvent----> MidiBridge::handle ---------------> AudioController
//!   set_feedback <--(target,value)-- MidiBridge::feedback <--CoreEvent-- audio.subscribe()
//!   monitor() / learned() / status_changes() ----> MidiEvent ----------> shell (UI events)
//! ```
//!
//! The worker finds the device by port name patterns and handles hot-plug
//! itself. "Auto-connect" means the worker is started at launch; a manual
//! device replaces the mapping's input and output patterns with that name.
//! Overrides are stored in the library settings (key `midi.settings`) and
//! applied to the shipped mapping with `Mapping::apply_input_overrides`;
//! changing them restarts the worker with the new mapping.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TrySendError, never, select};
use opendeck_contracts::control::ControlAction;
use opendeck_contracts::deck::DeckState;
use opendeck_contracts::mixer::MixerState;
use opendeck_library::Library;
use opendeck_midi::controller::BackendFactory;
use opendeck_midi::learn::{LearnCapture, LearnHint};
use opendeck_midi::mapping::{Mapping, MidiInput, ValueSpec};
use opendeck_midi::monitor::{Direction, MonitorEvent, MonitorLabel};
use opendeck_midi::{Controller, ControllerConfig, ControllerStatus, backend};

use crate::midi_bridge::{BridgeNotice, MidiBridge};
use crate::midi_types::{
    BrowseContextDto, BrowseCursorDto, MidiBindingDto, MidiLearnResultDto, MidiMappingRowDto,
    MidiMonitorDirection, MidiMonitorEntryDto, MidiMonitorLabelDto, MidiOverrideDto, MidiPortDto,
    MidiSettingsDto, MidiStatusDto, PadModeChanged,
};
use crate::pads::PadModes;
use crate::types::{CoreError, CoreEvent};
use crate::{AudioController, LibraryService};

/// Key of [`MidiSettingsDto`] in the library settings table.
pub const MIDI_SETTINGS_KEY: &str = "midi.settings";

/// Monitor entries are sent to the UI in batches at most this often.
const MONITOR_FLUSH: Duration = Duration::from_millis(50);

/// Entries kept while the UI is slow to take them.
const MONITOR_BACKLOG: usize = 2000;

/// Deck, mixer and FX state is re-read this often for the LEDs.
const REFRESH: Duration = Duration::from_millis(250);

/// Capacity of each event subscriber queue.
const SUBSCRIBER_QUEUE: usize = 1024;

/// Makes a fresh backend factory for each worker start.
pub type BackendMaker = Arc<dyn Fn() -> BackendFactory + Send + Sync>;

/// What the service needs from the application.
pub struct MidiServiceOptions {
    /// The shipped mapping.
    pub mapping: Mapping,
    /// Audio control handle.
    pub audio: AudioController,
    /// Library, for browsing and loading.
    pub library: Option<LibraryService>,
    /// Pad modes shared with the shell.
    pub pad_modes: PadModes,
    /// Where settings and overrides persist (the library's settings table).
    pub store: Option<Library>,
    /// Replaces the system MIDI backend (tests use the mock).
    pub backend: Option<BackendMaker>,
    /// Port rescan period; `None` keeps the worker default.
    pub rescan_interval: Option<Duration>,
}

/// Something the shell forwards to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum MidiEvent {
    /// Connection state changed.
    Status(MidiStatusDto),
    /// A batch of monitored messages.
    Monitor(Vec<MidiMonitorEntryDto>),
    /// A learn gesture finished.
    Learn(MidiLearnResultDto),
    /// The browse encoder moved the cursor.
    Browse(BrowseCursorDto),
    /// A pad mode button changed a deck's mode.
    PadMode(PadModeChanged),
    /// The mixer changed (a knob or fader on the controller moved).
    Mixer(Box<MixerState>),
    /// A deck's control state changed (tempo, sync, vinyl, ...).
    Deck(Box<DeckState>),
}

type Job = Box<dyn FnOnce(&mut Worker) + Send>;

enum Cmd {
    Run(Job),
    Shutdown,
}

struct Guard {
    tx: Sender<Cmd>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Ok(thread) = self.thread.get_mut()
            && let Some(thread) = thread.take()
        {
            let _ = thread.join();
        }
    }
}

/// Cloneable handle to the MIDI service thread. Dropping the last clone stops
/// the thread and the controller worker.
#[derive(Clone)]
pub struct MidiService {
    tx: Sender<Cmd>,
    _guard: Arc<Guard>,
}

impl std::fmt::Debug for MidiService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiService").finish_non_exhaustive()
    }
}

impl MidiService {
    /// Starts the service thread. When `auto_connect` is stored as on (the
    /// default) the controller worker starts at once.
    pub fn start(opts: MidiServiceOptions) -> std::io::Result<Self> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let core_rx = opts.audio.subscribe().unwrap_or_else(|_| never());
        let thread = std::thread::Builder::new()
            .name("opendeck-midi".into())
            .spawn(move || {
                let mut worker = Worker::new(opts, core_rx);
                worker.run(rx);
            })?;
        Ok(Self {
            tx: tx.clone(),
            _guard: Arc::new(Guard {
                tx,
                thread: Mutex::new(Some(thread)),
            }),
        })
    }

    fn ask<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Worker) -> T + Send + 'static,
    ) -> Result<T, CoreError> {
        let (reply, answer) = crossbeam_channel::bounded(1);
        self.tx
            .send(Cmd::Run(Box::new(move |w| {
                let _ = reply.send(f(w));
            })))
            .map_err(|_| CoreError::NotRunning)?;
        answer
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| CoreError::NotRunning)
    }

    /// Events for the UI. The queue is bounded; a slow reader loses events.
    pub fn subscribe(&self) -> Result<Receiver<MidiEvent>, CoreError> {
        let (tx, rx) = crossbeam_channel::bounded(SUBSCRIBER_QUEUE);
        // Waits for the thread, so events after this call are not missed.
        self.ask(move |w| w.subscribers.push(tx))?;
        Ok(rx)
    }

    /// MIDI ports the system reports, with the ones the mapping would use.
    pub fn list_ports(&self) -> Result<Vec<MidiPortDto>, CoreError> {
        self.ask(|w| w.list_ports())?
    }

    /// Current connection state.
    pub fn status(&self) -> Result<MidiStatusDto, CoreError> {
        self.ask(|w| w.status_dto())
    }

    /// Starts the worker. `device` selects a port by name and is remembered;
    /// `None` follows the mapping's device patterns.
    pub fn connect(&self, device: Option<String>) -> Result<MidiStatusDto, CoreError> {
        self.ask(move |w| w.connect(device))?
    }

    /// Stops the worker and closes the ports.
    pub fn disconnect(&self) -> Result<MidiStatusDto, CoreError> {
        self.ask(|w| w.disconnect())
    }

    /// Start the worker at launch.
    pub fn set_auto_connect(&self, on: bool) -> Result<MidiStatusDto, CoreError> {
        self.ask(move |w| {
            w.settings.auto_connect = on;
            w.save();
            w.emit_status();
            w.status_dto()
        })
    }

    /// Hardware fader start plays and pauses decks.
    pub fn set_fader_start(&self, on: bool) -> Result<MidiStatusDto, CoreError> {
        self.ask(move |w| {
            w.settings.fader_start = on;
            w.bridge.set_fader_start(on);
            w.save();
            w.emit_status();
            w.status_dto()
        })
    }

    /// The stored MIDI settings.
    pub fn settings(&self) -> Result<MidiSettingsDto, CoreError> {
        self.ask(|w| w.settings.clone())
    }

    /// Starts or stops the monitor stream.
    pub fn set_monitor(&self, on: bool) -> Result<(), CoreError> {
        self.ask(move |w| w.set_monitor(on))?
    }

    /// Arms MIDI learn for `control_id`; the result arrives as
    /// [`MidiEvent::Learn`].
    pub fn learn_start(&self, control_id: String) -> Result<(), CoreError> {
        self.ask(move |w| w.learn_start(control_id))?
    }

    /// Disarms MIDI learn.
    pub fn learn_cancel(&self) -> Result<(), CoreError> {
        self.ask(|w| w.learn_cancel())
    }

    /// Rows of the mapping table with overrides applied.
    pub fn mapping_rows(&self) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        self.ask(|w| w.rows())
    }

    /// Binds a control to `binding` and stores the override.
    pub fn set_override(
        &self,
        control_id: String,
        binding: MidiBindingDto,
    ) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        self.ask(move |w| w.set_override(&control_id, binding))?
    }

    /// Restores one control to the shipped binding.
    pub fn clear_override(&self, control_id: String) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        self.ask(move |w| w.clear_overrides(Some(&control_id)))?
    }

    /// Restores every control to the shipped bindings.
    pub fn reset_overrides(&self) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        self.ask(|w| w.clear_overrides(None))?
    }

    /// Writes the effective mapping (with overrides) as JSON.
    pub fn export_mapping(&self, path: String) -> Result<(), CoreError> {
        self.ask(move |w| w.export(Path::new(&path)))?
    }

    /// Reads a mapping JSON of the same mapping id and stores its bindings as
    /// overrides.
    pub fn import_mapping(&self, path: String) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        self.ask(move |w| w.import(Path::new(&path)))?
    }

    /// The list the library pane shows and its selected row.
    pub fn set_browse_context(&self, context: BrowseContextDto) -> Result<(), CoreError> {
        self.ask(move |w| w.bridge.set_browse_context(context))
    }

    /// The pane selected a row.
    pub fn browse_select(&self, index: u32) -> Result<(), CoreError> {
        self.ask(move |w| w.bridge.browse_select(index))
    }
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::Invalid(message.into())
}

/// Builds the effective mapping: the shipped one with every override that
/// still applies. Overrides for unknown controls or that now clash with
/// another input are skipped with a warning.
fn effective_mapping(base: &Mapping, overrides: &[MidiOverrideDto]) -> Mapping {
    let mut mapping = base.clone();
    for item in overrides {
        let mut trial = mapping.clone();
        let input: MidiInput = item.binding.into();
        match trial.apply_input_overrides(&[(item.control_id.clone(), input)]) {
            Ok(unknown) if unknown.is_empty() => mapping = trial,
            Ok(_) => {
                tracing::warn!(control = %item.control_id, "MIDI override for an unknown control")
            }
            Err(err) => {
                tracing::warn!(control = %item.control_id, %err, "MIDI override skipped");
            }
        }
    }
    mapping
}

struct ControlInfo {
    label: Option<String>,
    action: ControlAction,
}

struct Worker {
    base: Mapping,
    mapping: Mapping,
    info: HashMap<String, ControlInfo>,
    settings: MidiSettingsDto,
    store: Option<Library>,
    backend: Option<BackendMaker>,
    rescan: Option<Duration>,
    controller: Option<Controller>,
    bridge: MidiBridge,
    core_rx: Receiver<CoreEvent>,
    subscribers: Vec<Sender<MidiEvent>>,
    monitor_on: bool,
    monitor_buf: Vec<MidiMonitorEntryDto>,
    monitor_flushed: Instant,
    learning: Option<String>,
    last_status: Option<MidiStatusDto>,
    last_mixer: Option<MixerState>,
    last_decks: Vec<DeckState>,
    refreshed: Instant,
}

impl Worker {
    fn new(opts: MidiServiceOptions, core_rx: Receiver<CoreEvent>) -> Self {
        let settings = opts
            .store
            .as_ref()
            .and_then(|s| {
                s.get_setting_json::<MidiSettingsDto>(MIDI_SETTINGS_KEY)
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
        let mapping = effective_mapping(&opts.mapping, &settings.overrides);
        let mut bridge = MidiBridge::new(opts.audio, opts.library, opts.pad_modes);
        bridge.set_fader_start(settings.fader_start);
        let mut worker = Self {
            info: control_info(&mapping),
            base: opts.mapping,
            mapping,
            settings,
            store: opts.store,
            backend: opts.backend,
            rescan: opts.rescan_interval,
            controller: None,
            bridge,
            core_rx,
            subscribers: Vec::new(),
            monitor_on: false,
            monitor_buf: Vec::new(),
            monitor_flushed: Instant::now(),
            learning: None,
            last_status: None,
            last_mixer: None,
            last_decks: Vec::new(),
            refreshed: Instant::now(),
        };
        if worker.settings.auto_connect
            && let Err(err) = worker.start_controller()
        {
            tracing::warn!(%err, "MIDI worker did not start");
        }
        worker
    }

    fn run(&mut self, commands: Receiver<Cmd>) {
        loop {
            let (ctl_events, ctl_status, ctl_monitor, ctl_learn) = match &self.controller {
                Some(c) => (
                    c.events().clone(),
                    c.status_changes().clone(),
                    c.monitor().clone(),
                    c.learned().clone(),
                ),
                None => (never(), never(), never(), never()),
            };
            select! {
                recv(commands) -> cmd => match cmd {
                    Ok(Cmd::Run(job)) => job(self),
                    Ok(Cmd::Shutdown) | Err(_) => break,
                },
                recv(self.core_rx) -> event => match event {
                    Ok(event) => {
                        self.bridge.observe(&event);
                        self.push_feedback();
                    }
                    Err(_) => self.core_rx = never(),
                },
                recv(ctl_events) -> event => {
                    if let Ok(event) = event {
                        let notices = self.bridge.handle(&event);
                        for notice in notices {
                            match notice {
                                BridgeNotice::PadMode(p) => self.emit(&MidiEvent::PadMode(p)),
                                BridgeNotice::Browse(b) => self.emit(&MidiEvent::Browse(b)),
                            }
                        }
                        self.push_feedback();
                        self.emit_state_changes();
                    }
                },
                recv(ctl_status) -> status => {
                    if status.is_ok() {
                        self.on_status();
                    }
                },
                recv(ctl_monitor) -> entry => {
                    if let Ok(entry) = entry {
                        self.buffer_monitor(&entry);
                    }
                },
                recv(ctl_learn) -> capture => {
                    if let Ok(capture) = capture {
                        self.on_learned(capture);
                    }
                },
                default(Duration::from_millis(25)) => {},
            }
            self.tick();
        }
        if let Some(controller) = self.controller.take() {
            controller.shutdown();
        }
    }

    fn tick(&mut self) {
        if self.monitor_flushed.elapsed() >= MONITOR_FLUSH {
            self.flush_monitor();
        }
        if self.controller.is_some() && self.refreshed.elapsed() >= REFRESH {
            self.refreshed = Instant::now();
            self.bridge.refresh();
            self.push_feedback();
            self.emit_state_changes();
        }
    }

    /// Tells the UI about mixer and deck changes the bridge noticed (moves on
    /// the controller do not pass through the UI).
    fn emit_state_changes(&mut self) {
        if self.controller.is_none() {
            return;
        }
        if let Some(mixer) = self.bridge.mixer().copied()
            && self.last_mixer.as_ref() != Some(&mixer)
        {
            self.last_mixer = Some(mixer);
            self.emit(&MidiEvent::Mixer(Box::new(mixer)));
        }
        let decks: Vec<DeckState> = self.bridge.decks().cloned().collect();
        for state in &decks {
            let known = self.last_decks.iter().find(|d| d.deck == state.deck);
            if known != Some(state) {
                self.emit(&MidiEvent::Deck(Box::new(state.clone())));
            }
        }
        self.last_decks = decks;
    }

    // ----- controller -----

    fn start_controller(&mut self) -> Result<(), CoreError> {
        self.stop_controller();
        let mut mapping = self.mapping.clone();
        if let Some(device) = &self.settings.device {
            mapping.devices.input = vec![device.clone()];
            mapping.devices.output = vec![device.clone()];
        }
        let mut config = ControllerConfig::new(mapping);
        if let Some(rescan) = self.rescan {
            config.rescan_interval = rescan;
        }
        let controller = match &self.backend {
            Some(make) => Controller::start_with(config, make()),
            None => Controller::start(config),
        }
        .map_err(|e| CoreError::Invalid(e.to_string()))?;
        if self.monitor_on {
            let _ = controller.set_monitor(true);
        }
        self.controller = Some(controller);
        self.bridge.reset_feedback();
        self.bridge.refresh();
        self.push_feedback();
        self.emit_status();
        Ok(())
    }

    fn stop_controller(&mut self) {
        if let Some(controller) = self.controller.take() {
            controller.shutdown();
        }
        self.learning = None;
    }

    fn connect(&mut self, device: Option<String>) -> Result<MidiStatusDto, CoreError> {
        self.settings.device = device;
        self.save();
        self.start_controller()?;
        Ok(self.status_dto())
    }

    fn disconnect(&mut self) -> MidiStatusDto {
        self.stop_controller();
        self.emit_status();
        self.status_dto()
    }

    fn on_status(&mut self) {
        let connected = self
            .controller
            .as_ref()
            .is_some_and(|c| c.status().input.is_some());
        let was_connected = self.last_status.as_ref().is_some_and(|s| s.connected);
        if connected && !was_connected {
            // The device just appeared: light everything for the current state.
            self.bridge.reset_feedback();
            self.bridge.refresh();
            self.push_feedback();
            if let Some(c) = &self.controller {
                let _ = c.resend_feedback();
            }
        }
        self.emit_status();
    }

    fn push_feedback(&mut self) {
        let Some(controller) = &self.controller else {
            return;
        };
        for (target, value) in self.bridge.feedback() {
            let _ = controller.set_feedback(target, value);
        }
    }

    // ----- status, ports -----

    fn controller_status(&self) -> ControllerStatus {
        self.controller
            .as_ref()
            .map(Controller::status)
            .unwrap_or_default()
    }

    fn status_dto(&self) -> MidiStatusDto {
        let status = self.controller_status();
        MidiStatusDto {
            mapping_id: self.mapping.id.clone(),
            mapping_name: self.mapping.name.clone(),
            enabled: self.controller.is_some(),
            connected: status.input.is_some(),
            input: status.input,
            output: status.output,
            last_error: status.last_error,
            auto_connect: self.settings.auto_connect,
            device: self.settings.device.clone(),
            fader_start: self.settings.fader_start,
            dropped_input: status.dropped_input,
            verify_count: u32::try_from(self.mapping.verify_count()).unwrap_or(u32::MAX),
        }
    }

    fn emit_status(&mut self) {
        let status = self.status_dto();
        if self.last_status.as_ref() != Some(&status) {
            self.last_status = Some(status.clone());
            self.emit(&MidiEvent::Status(status));
        }
    }

    fn list_ports(&self) -> Result<Vec<MidiPortDto>, CoreError> {
        let to_err = |e: opendeck_midi::MidiError| CoreError::Invalid(e.to_string());
        let inputs = backend::list_input_ports().map_err(to_err)?;
        let outputs = backend::list_output_ports().map_err(to_err)?;
        let mut names: Vec<String> = inputs
            .iter()
            .chain(&outputs)
            .map(|p| p.name.clone())
            .collect();
        names.sort();
        names.dedup();
        let devices = &self.mapping.devices;
        Ok(names
            .into_iter()
            .map(|name| MidiPortDto {
                has_input: inputs.iter().any(|p| p.name == name),
                has_output: outputs.iter().any(|p| p.name == name),
                matches_mapping: backend::matches_any(&name, &devices.input),
                name,
            })
            .collect())
    }

    // ----- monitor, learn -----

    fn set_monitor(&mut self, on: bool) -> Result<(), CoreError> {
        self.monitor_on = on;
        if !on {
            self.monitor_buf.clear();
        }
        match &self.controller {
            Some(c) => c
                .set_monitor(on)
                .map_err(|e| CoreError::Invalid(e.to_string())),
            None => Ok(()),
        }
    }

    fn buffer_monitor(&mut self, entry: &MonitorEvent) {
        if !self.monitor_on {
            return;
        }
        let label = match &entry.label {
            MonitorLabel::Output => MidiMonitorLabelDto::Output,
            MonitorLabel::Control { id, verify } => {
                let info = self.info.get(id);
                MidiMonitorLabelDto::Control {
                    id: id.clone(),
                    label: info.and_then(|i| i.label.clone()),
                    action: info.map(|i| i.action),
                    verify: verify.clone(),
                }
            }
            MonitorLabel::Modifier { id, verify } => MidiMonitorLabelDto::Modifier {
                id: id.clone(),
                verify: verify.clone(),
            },
            MonitorLabel::Pending { id } => MidiMonitorLabelDto::Pending { id: id.clone() },
            MonitorLabel::KnownUnmapped { source_ref, reason } => {
                MidiMonitorLabelDto::KnownUnmapped {
                    source_ref: source_ref.clone(),
                    reason: reason.clone(),
                }
            }
            MonitorLabel::Unmapped => MidiMonitorLabelDto::Unmapped,
            MonitorLabel::Ignored => MidiMonitorLabelDto::Ignored,
        };
        if self.monitor_buf.len() >= MONITOR_BACKLOG {
            self.monitor_buf.remove(0);
        }
        self.monitor_buf.push(MidiMonitorEntryDto {
            ts_us: entry.ts_us,
            direction: match entry.direction {
                Direction::In => MidiMonitorDirection::In,
                Direction::Out => MidiMonitorDirection::Out,
            },
            bytes: entry.bytes.clone(),
            text: entry.text.clone(),
            label,
        });
    }

    fn flush_monitor(&mut self) {
        self.monitor_flushed = Instant::now();
        if self.monitor_buf.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut self.monitor_buf);
        self.emit(&MidiEvent::Monitor(batch));
    }

    fn learn_start(&mut self, control_id: String) -> Result<(), CoreError> {
        let Some(control) = self.mapping.controls.iter().find(|c| c.id == control_id) else {
            return Err(invalid(format!("unknown control {control_id}")));
        };
        let hint = match control.value {
            ValueSpec::Button { .. } => LearnHint::Button,
            ValueSpec::Absolute { .. } => LearnHint::Absolute,
            ValueSpec::Relative { encoding, .. } => LearnHint::Relative(encoding),
        };
        let Some(controller) = &self.controller else {
            return Err(invalid("the controller is not connected"));
        };
        controller
            .start_learn(hint)
            .map_err(|e| CoreError::Invalid(e.to_string()))?;
        self.learning = Some(control_id);
        Ok(())
    }

    fn learn_cancel(&mut self) {
        self.learning = None;
        if let Some(controller) = &self.controller {
            let _ = controller.cancel_learn();
        }
    }

    fn on_learned(&mut self, capture: LearnCapture) {
        let Some(control_id) = self.learning.take() else {
            return;
        };
        let binding = MidiBindingDto::from(capture.midi);
        let result = match self.set_override(&control_id, binding) {
            Ok(_) => MidiLearnResultDto {
                control_id,
                binding: Some(binding),
                description: capture.description,
                error: None,
            },
            Err(err) => MidiLearnResultDto {
                control_id,
                binding: None,
                description: capture.description,
                error: Some(err.to_string()),
            },
        };
        self.emit(&MidiEvent::Learn(result));
    }

    // ----- mapping -----

    fn rows(&self) -> Vec<MidiMappingRowDto> {
        let base: HashMap<&str, MidiInput> = self
            .base
            .controls
            .iter()
            .map(|c| (c.id.as_str(), c.midi))
            .collect();
        self.mapping
            .controls
            .iter()
            .map(|c| {
                let layers: Vec<&str> = c
                    .when
                    .iter()
                    .filter(|(_, held)| **held)
                    .map(|(k, _)| k.as_str())
                    .collect();
                MidiMappingRowDto {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    source_ref: c.source_ref.clone(),
                    binding: c.midi.into(),
                    action: c.action,
                    layer: if layers.is_empty() {
                        "base".to_owned()
                    } else {
                        layers.join("+")
                    },
                    value_kind: match c.value {
                        ValueSpec::Button { .. } => "button",
                        ValueSpec::Absolute { .. } => "absolute",
                        ValueSpec::Relative { .. } => "relative",
                    }
                    .to_owned(),
                    verify: c.verify.clone(),
                    overridden: base.get(c.id.as_str()).is_some_and(|b| *b != c.midi),
                }
            })
            .collect()
    }

    /// Replaces the override list, rebuilds the mapping, restarts the worker
    /// and stores the result. The old state stays if the new list is invalid.
    fn apply_overrides(&mut self, overrides: Vec<MidiOverrideDto>) -> Result<(), CoreError> {
        // Validate strictly: a user change must not be skipped silently.
        let mut trial = self.base.clone();
        let list: Vec<(String, MidiInput)> = overrides
            .iter()
            .map(|o| (o.control_id.clone(), o.binding.into()))
            .collect();
        let unknown = trial
            .apply_input_overrides(&list)
            .map_err(|e| invalid(e.to_string()))?;
        if let Some(id) = unknown.first() {
            return Err(invalid(format!("unknown control {id}")));
        }
        self.settings.overrides = overrides;
        self.mapping = trial;
        self.info = control_info(&self.mapping);
        self.save();
        if self.controller.is_some() {
            self.start_controller()?;
        }
        Ok(())
    }

    fn set_override(
        &mut self,
        control_id: &str,
        binding: MidiBindingDto,
    ) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        let base_midi = self
            .base
            .controls
            .iter()
            .find(|c| c.id == control_id)
            .map(|c| c.midi)
            .ok_or_else(|| invalid(format!("unknown control {control_id}")))?;
        let mut overrides: Vec<MidiOverrideDto> = self
            .settings
            .overrides
            .iter()
            .filter(|o| o.control_id != control_id)
            .cloned()
            .collect();
        if MidiInput::from(binding) != base_midi {
            overrides.push(MidiOverrideDto {
                control_id: control_id.to_owned(),
                binding,
                base_version: self.base.version,
            });
        }
        self.apply_overrides(overrides)?;
        Ok(self.rows())
    }

    fn clear_overrides(
        &mut self,
        control_id: Option<&str>,
    ) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        let overrides = match control_id {
            Some(id) => self
                .settings
                .overrides
                .iter()
                .filter(|o| o.control_id != id)
                .cloned()
                .collect(),
            None => Vec::new(),
        };
        self.apply_overrides(overrides)?;
        Ok(self.rows())
    }

    fn export(&self, path: &Path) -> Result<(), CoreError> {
        let json = self.mapping.to_json().map_err(|e| invalid(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| invalid(format!("{}: {e}", path.display())))
    }

    fn import(&mut self, path: &Path) -> Result<Vec<MidiMappingRowDto>, CoreError> {
        let imported = Mapping::from_path(path).map_err(|e| invalid(e.to_string()))?;
        if imported.id != self.base.id {
            return Err(invalid(format!(
                "the file is for mapping {}, not {}",
                imported.id, self.base.id
            )));
        }
        let base: HashMap<&str, MidiInput> = self
            .base
            .controls
            .iter()
            .map(|c| (c.id.as_str(), c.midi))
            .collect();
        let overrides: Vec<MidiOverrideDto> = imported
            .controls
            .iter()
            .filter(|c| base.get(c.id.as_str()).is_some_and(|b| *b != c.midi))
            .map(|c| MidiOverrideDto {
                control_id: c.id.clone(),
                binding: c.midi.into(),
                base_version: self.base.version,
            })
            .collect();
        self.apply_overrides(overrides)?;
        Ok(self.rows())
    }

    // ----- plumbing -----

    fn save(&self) {
        if let Some(store) = &self.store
            && let Err(err) = store.set_setting_json(MIDI_SETTINGS_KEY, &self.settings)
        {
            tracing::warn!(%err, "could not store the MIDI settings");
        }
    }

    fn emit(&mut self, event: &MidiEvent) {
        self.subscribers
            .retain(|tx| match tx.try_send(event.clone()) {
                Ok(()) | Err(TrySendError::Full(_)) => true,
                Err(TrySendError::Disconnected(_)) => false,
            });
    }
}

fn control_info(mapping: &Mapping) -> HashMap<String, ControlInfo> {
    mapping
        .controls
        .iter()
        .map(|c| {
            (
                c.id.clone(),
                ControlInfo {
                    label: c.label.clone(),
                    action: c.action,
                },
            )
        })
        .collect()
}
