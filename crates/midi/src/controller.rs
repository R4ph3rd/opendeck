//! The MIDI worker: ports, hot-plug rescan, decoding and LED output on one
//! thread, with channels as the only interface.
//!
//! ```text
//! midir callback --(RawIn, bounded)--> worker thread --(ControlEvent)--> app-core
//!                                          ^   |
//!        set_feedback / learn / monitor ---+   +--> MIDI output (LEDs)
//! ```
//!
//! - The midir callback copies at most three bytes and a timestamp into a
//!   bounded channel with `try_send`. It never blocks, allocates or locks.
//! - The worker owns the [`Pipeline`] (decoding, mapping, feedback policy),
//!   both port connections and the rescan timer. It rescans ports every
//!   `rescan_interval`, opens the ports a mapping's `devices` patterns match,
//!   closes them when the device disappears and reopens them when it returns.
//!   On every output (re)connect it sends `on_connect` and replays the cached
//!   LED state.
//! - Nothing here touches the audio thread. app-core calls
//!   [`Controller::set_feedback`] from the control thread and reads
//!   [`Controller::events`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select, unbounded};
use opendeck_contracts::control::{ControlEvent, FeedbackTarget, FeedbackValue};

use crate::backend::{
    InputHandle, MidiBackend, MidirBackend, OutputPort, PortInfo, RawSink, matches_any,
};
use crate::error::MidiError;
use crate::feedback::{FeedbackConfig, OutMessage};
use crate::input::Handled;
use crate::learn::{LearnCapture, LearnHint, LearnSession};
use crate::mapping::Mapping;
use crate::monitor::{MonitorEvent, MonitorLabeler};
use crate::pipeline::Pipeline;

/// Controller settings.
#[derive(Debug, Clone)]
pub struct ControllerConfig {
    /// The mapping to run.
    pub mapping: Mapping,
    /// LED rate limits.
    pub feedback: FeedbackConfig,
    /// Port rescan period (hot-plug).
    pub rescan_interval: Duration,
    /// Capacity of the raw input, event and monitor channels.
    pub channel_capacity: usize,
    /// midir client name.
    pub client_name: String,
}

impl ControllerConfig {
    /// Defaults: 1 s rescan, 4096-message channels.
    pub fn new(mapping: Mapping) -> Self {
        Self {
            mapping,
            feedback: FeedbackConfig::default(),
            rescan_interval: Duration::from_secs(1),
            channel_capacity: 4096,
            client_name: "OpenDeck".to_owned(),
        }
    }
}

/// Connection state, readable at any time and also pushed on every change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControllerStatus {
    /// Name of the open input port.
    pub input: Option<String>,
    /// Name of the open output port.
    pub output: Option<String>,
    /// Last open or send error, cleared on the next success.
    pub last_error: Option<String>,
    /// Input messages dropped because the raw channel was full or the message
    /// was longer than three bytes (SysEx).
    pub dropped_input: u64,
    /// Control events dropped because the event channel was full.
    pub dropped_events: u64,
}

enum Command {
    Feedback(FeedbackTarget, FeedbackValue),
    ResendAll,
    SetMonitor(bool),
    Learn(Option<LearnHint>),
    Shutdown,
}

/// A message copied out of the midir callback.
#[derive(Clone, Copy)]
struct RawIn {
    driver_ts_us: u64,
    arrival_us: u64,
    len: u8,
    data: [u8; 3],
}

/// Handle to the running worker. Dropping it stops the thread.
pub struct Controller {
    events: Receiver<ControlEvent>,
    status_rx: Receiver<ControllerStatus>,
    monitor_rx: Receiver<MonitorEvent>,
    learn_rx: Receiver<LearnCapture>,
    commands: Sender<Command>,
    status: Arc<Mutex<ControllerStatus>>,
    thread: Option<JoinHandle<()>>,
}

/// Creates the backend on the worker thread.
pub type BackendFactory = Box<dyn FnOnce() -> Result<Box<dyn MidiBackend>, MidiError> + Send>;

impl Controller {
    /// Starts the worker on the system MIDI backend.
    pub fn start(config: ControllerConfig) -> Result<Self, MidiError> {
        let name = config.client_name.clone();
        Self::start_with(
            config,
            Box::new(move || Ok(Box::new(MidirBackend::new(&name)) as Box<dyn MidiBackend>)),
        )
    }

    /// Starts the worker with a custom backend (tests use the mock).
    pub fn start_with(
        config: ControllerConfig,
        backend: BackendFactory,
    ) -> Result<Self, MidiError> {
        let pipeline = Pipeline::new(config.mapping.clone(), config.feedback)
            .map_err(|e| MidiError::Backend(e.to_string()))?;
        let cap = config.channel_capacity.max(16);
        let (events_tx, events) = bounded(cap);
        let (status_tx, status_rx) = bounded(64);
        let (monitor_tx, monitor_rx) = bounded(cap);
        let (learn_tx, learn_rx) = bounded(4);
        let (cmd_tx, cmd_rx) = unbounded();
        let (raw_tx, raw_rx) = bounded::<RawIn>(cap);
        let status = Arc::new(Mutex::new(ControllerStatus::default()));
        let dropped_input = Arc::new(AtomicU64::new(0));

        let worker_status = Arc::clone(&status);
        let spawned = move || {
            let worker = Worker {
                labeler: MonitorLabeler::new(&config.mapping),
                pipeline,
                config,
                backend: None,
                input: None,
                output: None,
                events_tx,
                status_tx,
                monitor_tx,
                learn_tx,
                raw_tx,
                raw_rx,
                cmd_rx,
                status: worker_status,
                dropped_input,
                dropped_events: 0,
                monitor_on: false,
                learner: None,
                start: Instant::now(),
                clock: ClockMap::default(),
                next_rescan: Instant::now(),
                out_buf: Vec::new(),
                event_buf: Vec::new(),
                last_published: ControllerStatus::default(),
            };
            worker.run(backend);
        };
        let thread = std::thread::Builder::new()
            .name("opendeck-midi".into())
            .spawn(spawned)
            .map_err(|e| MidiError::Backend(e.to_string()))?;
        Ok(Self {
            events,
            status_rx,
            monitor_rx,
            learn_rx,
            commands: cmd_tx,
            status,
            thread: Some(thread),
        })
    }

    /// Decoded control events, in order.
    pub fn events(&self) -> &Receiver<ControlEvent> {
        &self.events
    }

    /// Status snapshots, one per change.
    pub fn status_changes(&self) -> &Receiver<ControllerStatus> {
        &self.status_rx
    }

    /// The current status.
    pub fn status(&self) -> ControllerStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Raw traffic with timestamps; empty until [`Controller::set_monitor`].
    pub fn monitor(&self) -> &Receiver<MonitorEvent> {
        &self.monitor_rx
    }

    /// Captured MIDI learn gestures.
    pub fn learned(&self) -> &Receiver<LearnCapture> {
        &self.learn_rx
    }

    /// Sets the state of an LED or meter target.
    pub fn set_feedback(
        &self,
        target: FeedbackTarget,
        value: FeedbackValue,
    ) -> Result<(), MidiError> {
        self.send(Command::Feedback(target, value))
    }

    /// Sends all cached LED state again.
    pub fn resend_feedback(&self) -> Result<(), MidiError> {
        self.send(Command::ResendAll)
    }

    /// Starts or stops the monitor stream.
    pub fn set_monitor(&self, on: bool) -> Result<(), MidiError> {
        self.send(Command::SetMonitor(on))
    }

    /// Arms MIDI learn: while armed, input is captured instead of mapped.
    pub fn start_learn(&self, hint: LearnHint) -> Result<(), MidiError> {
        self.send(Command::Learn(Some(hint)))
    }

    /// Disarms MIDI learn without a capture.
    pub fn cancel_learn(&self) -> Result<(), MidiError> {
        self.send(Command::Learn(None))
    }

    /// Stops the worker and waits for it.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn send(&self, command: Command) -> Result<(), MidiError> {
        self.commands
            .send(command)
            .map_err(|_| MidiError::NotRunning)
    }

    fn stop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Maps the driver clock of one connection onto the worker clock: the first
/// message anchors the offset, later messages keep the driver's spacing.
#[derive(Default)]
struct ClockMap {
    offset: Option<i128>,
    last: u64,
}

impl ClockMap {
    fn reset(&mut self) {
        self.offset = None;
    }

    fn local_time(&mut self, driver_ts_us: u64, arrival_us: u64) -> u64 {
        let offset = *self
            .offset
            .get_or_insert(i128::from(arrival_us) - i128::from(driver_ts_us));
        let local = u64::try_from((i128::from(driver_ts_us) + offset).max(0)).unwrap_or(0);
        self.last = self.last.max(local);
        self.last
    }
}

struct Worker {
    labeler: MonitorLabeler,
    pipeline: Pipeline,
    config: ControllerConfig,
    backend: Option<Box<dyn MidiBackend>>,
    input: Option<(PortInfo, InputHandle)>,
    output: Option<(PortInfo, Box<dyn OutputPort>)>,
    events_tx: Sender<ControlEvent>,
    status_tx: Sender<ControllerStatus>,
    monitor_tx: Sender<MonitorEvent>,
    learn_tx: Sender<LearnCapture>,
    raw_tx: Sender<RawIn>,
    raw_rx: Receiver<RawIn>,
    cmd_rx: Receiver<Command>,
    status: Arc<Mutex<ControllerStatus>>,
    dropped_input: Arc<AtomicU64>,
    dropped_events: u64,
    monitor_on: bool,
    learner: Option<LearnSession>,
    start: Instant,
    clock: ClockMap,
    next_rescan: Instant,
    out_buf: Vec<OutMessage>,
    event_buf: Vec<ControlEvent>,
    last_published: ControllerStatus,
}

impl Worker {
    fn now_us(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_micros()).unwrap_or(u64::MAX)
    }

    fn run(mut self, backend: BackendFactory) {
        match backend() {
            Ok(b) => self.backend = Some(b),
            Err(e) => {
                self.set_error(Some(e.to_string()));
                // Stay alive so commands keep draining and the handle can be dropped.
                self.drain_commands_until_shutdown();
                return;
            }
        }
        let raw_rx = self.raw_rx.clone();
        let cmd_rx = self.cmd_rx.clone();
        loop {
            let timeout = self.next_timeout();
            select! {
                recv(raw_rx) -> msg => {
                    if let Ok(raw) = msg {
                        self.on_raw(raw);
                    }
                }
                recv(cmd_rx) -> cmd => match cmd {
                    Ok(Command::Shutdown) | Err(_) => break,
                    Ok(cmd) => self.on_command(cmd),
                },
                default(timeout) => {}
            }
            self.housekeeping();
        }
        // Dropping the handles closes the ports.
        self.input = None;
        self.output = None;
    }

    fn drain_commands_until_shutdown(&self) {
        while let Ok(cmd) = self.cmd_rx.recv() {
            if matches!(cmd, Command::Shutdown) {
                break;
            }
        }
    }

    fn next_timeout(&self) -> Duration {
        let now = Instant::now();
        let mut timeout = self.next_rescan.saturating_duration_since(now);
        let now_us = self.now_us();
        let mut consider = |deadline_us: Option<u64>| {
            if let Some(d) = deadline_us {
                let wait = Duration::from_micros(d.saturating_sub(now_us));
                timeout = timeout.min(wait);
            }
        };
        consider(self.pipeline.next_deadline_us());
        consider(self.learner.as_ref().and_then(LearnSession::deadline_us));
        timeout.max(Duration::from_micros(200))
    }

    fn housekeeping(&mut self) {
        let now_us = self.now_us();
        self.pipeline
            .tick(now_us, &mut self.event_buf, &mut self.out_buf);
        if let Some(learner) = self.learner.as_mut()
            && let Some(capture) = learner.tick(now_us)
        {
            self.finish_learn(capture);
        }
        self.flush();
        if Instant::now() >= self.next_rescan {
            self.rescan();
            self.next_rescan = Instant::now() + self.config.rescan_interval;
        }
    }

    fn on_raw(&mut self, raw: RawIn) {
        let ts = self.clock.local_time(raw.driver_ts_us, raw.arrival_us);
        let bytes = &raw.data[..usize::from(raw.len)];
        if let Some(learner) = self.learner.as_mut() {
            let capture = learner.feed(ts, bytes);
            if self.monitor_on {
                let event = self.labeler.input(ts, bytes, &Handled::Unmapped);
                let _ = self.monitor_tx.try_send(event);
            }
            if let Some(capture) = capture {
                self.finish_learn(capture);
            }
            return;
        }
        let handled = self
            .pipeline
            .on_midi(ts, bytes, &mut self.event_buf, &mut self.out_buf);
        if self.monitor_on {
            let event = self.labeler.input(ts, bytes, &handled);
            let _ = self.monitor_tx.try_send(event);
        }
        self.flush();
    }

    fn finish_learn(&mut self, capture: LearnCapture) {
        self.learner = None;
        let _ = self.learn_tx.try_send(capture);
    }

    fn on_command(&mut self, cmd: Command) {
        let now_us = self.now_us();
        match cmd {
            Command::Feedback(target, value) => {
                self.pipeline
                    .set_feedback(target, value, now_us, &mut self.out_buf);
            }
            Command::ResendAll => self.pipeline.resend_all(now_us, &mut self.out_buf),
            Command::SetMonitor(on) => self.monitor_on = on,
            Command::Learn(Some(hint)) => self.learner = Some(LearnSession::new(hint)),
            Command::Learn(None) => self.learner = None,
            Command::Shutdown => {}
        }
        self.flush();
    }

    /// Delivers decoded events and sends queued output.
    fn flush(&mut self) {
        for event in self.event_buf.drain(..) {
            match self.events_tx.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => self.dropped_events += 1,
                Err(TrySendError::Disconnected(_)) => {}
            }
        }
        if self.out_buf.is_empty() {
            return;
        }
        let messages = std::mem::take(&mut self.out_buf);
        let ts = self.now_us();
        let mut failed = None;
        if let Some((_, port)) = self.output.as_mut() {
            for message in &messages {
                if let Err(e) = port.send(&message.bytes) {
                    failed = Some(e.to_string());
                    break;
                }
                if self.monitor_on {
                    let _ = self
                        .monitor_tx
                        .try_send(self.labeler.output(ts, &message.bytes));
                }
            }
        }
        // Messages produced while no output is open are not lost: the feedback
        // engine caches state and `resend_all` replays it on connect.
        if let Some(e) = failed {
            self.output = None;
            self.set_error(Some(e));
            self.publish_status();
        }
    }

    fn rescan(&mut self) {
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        let devices = self.config.mapping.devices.clone();
        let mut changed = false;

        match backend.input_ports() {
            Ok(ports) => {
                if let Some((open, _)) = &self.input
                    && !ports.contains(open)
                {
                    self.input = None;
                    self.clock.reset();
                    changed = true;
                }
                if self.input.is_none()
                    && let Some(port) = ports
                        .iter()
                        .find(|p| matches_any(&p.name, &devices.input))
                        .cloned()
                {
                    match backend.open_input(&port, self.sink()) {
                        Ok(handle) => {
                            self.input = Some((port, handle));
                            self.set_error(None);
                            changed = true;
                        }
                        Err(e) => self.set_error(Some(e.to_string())),
                    }
                }
            }
            Err(e) => self.set_error(Some(e.to_string())),
        }

        if !devices.output.is_empty() {
            match backend.output_ports() {
                Ok(ports) => {
                    if let Some((open, _)) = &self.output
                        && !ports.contains(open)
                    {
                        self.output = None;
                        changed = true;
                    }
                    if self.output.is_none()
                        && let Some(port) = ports
                            .iter()
                            .find(|p| matches_any(&p.name, &devices.output))
                            .cloned()
                    {
                        match backend.open_output(&port) {
                            Ok(handle) => {
                                self.output = Some((port, handle));
                                self.on_output_connected();
                                changed = true;
                            }
                            Err(e) => self.set_error(Some(e.to_string())),
                        }
                    }
                }
                Err(e) => self.set_error(Some(e.to_string())),
            }
        }
        self.backend = Some(backend);
        if changed {
            self.publish_status();
        }
    }

    /// `on_connect` messages first, then the cached LED state.
    fn on_output_connected(&mut self) {
        let now_us = self.now_us();
        self.out_buf.extend(self.pipeline.connect_messages());
        self.pipeline.resend_all(now_us, &mut self.out_buf);
        self.set_error(None);
        self.flush();
    }

    fn sink(&self) -> RawSink {
        let tx = self.raw_tx.clone();
        let dropped = Arc::clone(&self.dropped_input);
        let start = self.start;
        Box::new(move |driver_ts_us, bytes| {
            if bytes.len() > 3 {
                dropped.fetch_add(1, Ordering::Relaxed);
                return;
            }
            let mut data = [0u8; 3];
            data[..bytes.len()].copy_from_slice(bytes);
            let raw = RawIn {
                driver_ts_us,
                arrival_us: u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX),
                len: bytes.len() as u8,
                data,
            };
            if tx.try_send(raw).is_err() {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        })
    }

    fn set_error(&mut self, error: Option<String>) {
        let changed = {
            let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
            let changed = status.last_error != error;
            status.last_error = error;
            changed
        };
        if changed {
            self.publish_status();
        }
    }

    fn publish_status(&mut self) {
        let snapshot = {
            let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
            status.input = self.input.as_ref().map(|(p, _)| p.name.clone());
            status.output = self.output.as_ref().map(|(p, _)| p.name.clone());
            status.dropped_input = self.dropped_input.load(Ordering::Relaxed);
            status.dropped_events = self.dropped_events;
            status.clone()
        };
        if snapshot != self.last_published {
            self.last_published = snapshot.clone();
            let _ = self.status_tx.try_send(snapshot);
        }
    }
}
