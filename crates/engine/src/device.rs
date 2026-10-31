//! Audio device layer on cpal 0.18: list hosts and devices, open an output
//! stream around an [`EngineCore`] and, when mic or aux inputs are routed, an
//! input stream feeding the engine's input ring.
//!
//! Nothing here runs on the audio thread except the stream callbacks. The
//! output callback renders the core and copies its buses to their channel
//! pairs (see `routing`); the input callback copies the mic and aux channels
//! to the input ring (see `inputs`). Errors reported by the backend (device
//! unplugged, xruns) arrive on a backend thread and are published through
//! atomics; [`AudioOutput::poll_event`] turns them into device
//! [`EngineEvent`]s for the control thread. Callback timestamps give the
//! measured output and input latency ([`AudioOutput::output_latency`]).
//!
//! cpal has no duplex stream, so the input is always a second stream, even on
//! the same device. A lost input stream does not stop the output: it is
//! reported by [`AudioOutput::input_lost`] and
//! [`AudioOutput::reopen_input`] attaches a new one without touching the
//! engine.
//!
//! The `asio` cargo feature forwards to `cpal/asio` (Windows only), `jack` to
//! `cpal/jack`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, ErrorKind, FromSample, Host, HostId, SampleFormat, SizedSample, StreamConfig,
    SupportedBufferSize,
};
use opendeck_contracts::settings::{
    AudioHostKind, AudioSettings, ChannelPair, InputChannels, MAX_DEVICE_CHANNELS,
};
use opendeck_contracts::{EngineEvent, SampleRate};

use crate::audio::EngineCore;
use crate::handle::{EngineHandle, engine_with_xruns};
use crate::inputs::{INPUT_RING_FRAMES, InputProducer, InputShared};
use crate::routing::{
    InputChoice, InputMismatch, InputRouter, RangeInfo, Router, negotiate, negotiate_input,
};
use crate::{ConfigError, EngineConfig};

/// What can go wrong listing devices or opening a stream.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AudioError {
    /// The requested host API is not compiled in or not running.
    #[error("audio host unavailable: {0}")]
    HostUnavailable(String),
    /// The host reported an error while listing devices.
    #[error("could not list audio devices: {0}")]
    Devices(String),
    /// The host has no output device.
    #[error("no output device available")]
    NoDevice,
    /// No output device has the requested name on this host.
    #[error("output device not found: {0}")]
    DeviceNotFound(String),
    /// The device offers no usable configuration (needs 2+ channels in
    /// `f32`, `i32` or `i16`).
    #[error("device has no usable output configuration")]
    NoConfig,
    /// The stream could not be built.
    #[error("could not open the output stream: {0}")]
    Build(String),
    /// The stream was built but would not start.
    #[error("could not start the output stream: {0}")]
    Play(String),
    /// The engine configuration was rejected.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The host has no input device to use.
    #[error("no input device available")]
    NoInputDevice,
    /// No input device has the requested name on this host.
    #[error("input device not found: {0}")]
    InputNotFound(String),
    /// The input device cannot carry the routed channels at the output rate.
    #[error("input device has no configuration with {channels} channels at {rate} Hz")]
    InputConfig {
        /// Channels the routing needs.
        channels: u16,
        /// Sample rate the output runs at.
        rate: u32,
    },
    /// A mic or aux channel index is beyond the supported 32 channels.
    #[error("input channel is beyond channel {MAX_DEVICE_CHANNELS}")]
    InputChannel,
    /// The input stream could not be built or started.
    #[error("could not open the input stream: {0}")]
    InputBuild(String),
    /// The input ring was not available (a previous input stream still owns it).
    #[error("the input is still in use")]
    InputBusy,
}

/// What a host can do, for the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HostCaps {
    /// The host exposes at least one input device.
    pub input: bool,
    /// Some output device offers four channels or more.
    pub multichannel_output: bool,
    /// Some device reports a buffer size range.
    pub configurable_buffer: bool,
}

/// An audio host API (WASAPI, ASIO, CoreAudio, ALSA, JACK, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    /// Stable lower-case id from cpal (`"wasapi"`, `"alsa"`, ...).
    pub id: String,
    /// Display name.
    pub name: String,
    /// The settings enum for this host, `None` for hosts it does not model
    /// (for example PipeWire).
    pub kind: Option<AudioHostKind>,
    /// This is the platform default host.
    pub is_default: bool,
    /// What the host's devices offer.
    pub caps: HostCaps,
}

/// A device and what it supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Host id this device belongs to.
    pub host_id: String,
    /// Device name as the host reports it. Used to select the device.
    pub name: String,
    /// The host's default device in this direction.
    pub is_default: bool,
    /// Most channels any configuration offers.
    pub max_channels: u16,
    /// Default sample rate in Hz, if the host reports one.
    pub default_sample_rate: Option<u32>,
    /// Supported sample rate ranges `(min, max)` in Hz, deduplicated.
    pub sample_rate_ranges: Vec<(u32, u32)>,
    /// Supported buffer size range `(min, max)` in frames, if known.
    pub buffer_frames: Option<(u32, u32)>,
}

/// An output device and what it supports.
pub type OutputDeviceInfo = DeviceInfo;
/// An input device and what it supports.
pub type InputDeviceInfo = DeviceInfo;

/// Lists the hosts compiled in and running on this machine, with what their
/// devices offer. Enumerates every host's devices, so call it when the
/// settings screen opens, not per frame.
pub fn list_hosts() -> Vec<HostInfo> {
    let default = cpal::default_host().id();
    cpal::available_hosts()
        .into_iter()
        .map(|id| HostInfo {
            id: id.to_string(),
            name: id.name().to_string(),
            kind: host_kind(id),
            is_default: id == default,
            caps: host_caps(id),
        })
        .collect()
}

fn host_caps(id: HostId) -> HostCaps {
    let Ok(host) = cpal::host_from_id(id) else {
        return HostCaps::default();
    };
    let outputs = devices_of(&host, false).unwrap_or_default();
    let inputs = devices_of(&host, true).unwrap_or_default();
    HostCaps {
        input: !inputs.is_empty(),
        multichannel_output: outputs.iter().any(|d| d.max_channels >= 4),
        configurable_buffer: outputs.iter().any(|d| d.buffer_frames.is_some()),
    }
}

/// Lists output devices of `host` (`None` for the default host).
pub fn list_output_devices(host: Option<AudioHostKind>) -> Result<Vec<DeviceInfo>, AudioError> {
    devices_of(&resolve_host(host)?, false)
}

/// Lists input devices of `host` (`None` for the default host).
pub fn list_input_devices(host: Option<AudioHostKind>) -> Result<Vec<DeviceInfo>, AudioError> {
    devices_of(&resolve_host(host)?, true)
}

fn devices_of(host: &Host, input: bool) -> Result<Vec<DeviceInfo>, AudioError> {
    let default_name = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
    .and_then(|d| device_name(&d));
    let devices: Vec<cpal::Device> = if input {
        host.input_devices().map(Iterator::collect)
    } else {
        host.output_devices().map(Iterator::collect)
    }
    .map_err(|e| AudioError::Devices(e.to_string()))?;
    let host_id = host.id().to_string();
    Ok(devices
        .into_iter()
        .filter_map(|device| {
            let name = device_name(&device)?;
            let ranges = ranges_of(&device, input).ok()?;
            let mut sample_rate_ranges: Vec<(u32, u32)> =
                ranges.iter().map(|r| (r.min_rate, r.max_rate)).collect();
            sample_rate_ranges.sort_unstable();
            sample_rate_ranges.dedup();
            let buffer_frames = ranges
                .iter()
                .filter_map(|r| r.buffer)
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
            let default_config = if input {
                device.default_input_config()
            } else {
                device.default_output_config()
            };
            Some(DeviceInfo {
                host_id: host_id.clone(),
                is_default: default_name.as_deref() == Some(name.as_str()),
                max_channels: ranges.iter().map(|r| r.channels).max().unwrap_or(0),
                default_sample_rate: default_config.ok().map(|c| c.sample_rate()),
                sample_rate_ranges,
                buffer_frames,
                name,
            })
        })
        .collect())
}

fn ranges_of(device: &cpal::Device, input: bool) -> Result<Vec<RangeInfo>, AudioError> {
    let convert = |r: cpal::SupportedStreamConfigRange| RangeInfo {
        channels: r.channels(),
        min_rate: r.min_sample_rate(),
        max_rate: r.max_sample_rate(),
        buffer: match r.buffer_size() {
            SupportedBufferSize::Range { min, max } => Some((*min, *max)),
            SupportedBufferSize::Unknown => None,
        },
        format: r.sample_format(),
    };
    if input {
        device
            .supported_input_configs()
            .map(|configs| configs.map(convert).collect())
    } else {
        device
            .supported_output_configs()
            .map(|configs| configs.map(convert).collect())
    }
    .map_err(|e| AudioError::Devices(e.to_string()))
}

fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

fn host_kind(id: HostId) -> Option<AudioHostKind> {
    let name = id.name();
    [
        AudioHostKind::Wasapi,
        AudioHostKind::Asio,
        AudioHostKind::CoreAudio,
        AudioHostKind::Alsa,
        AudioHostKind::Jack,
    ]
    .into_iter()
    .find(|kind| host_name(*kind).eq_ignore_ascii_case(name))
}

fn host_name(kind: AudioHostKind) -> &'static str {
    match kind {
        AudioHostKind::Wasapi => "WASAPI",
        AudioHostKind::Asio => "ASIO",
        AudioHostKind::CoreAudio => "CoreAudio",
        AudioHostKind::Alsa => "ALSA",
        AudioHostKind::Jack => "JACK",
    }
}

fn resolve_host(kind: Option<AudioHostKind>) -> Result<Host, AudioError> {
    let Some(kind) = kind else {
        return Ok(cpal::default_host());
    };
    let wanted = host_name(kind);
    let id = cpal::available_hosts()
        .into_iter()
        .find(|id| id.name().eq_ignore_ascii_case(wanted))
        .ok_or_else(|| AudioError::HostUnavailable(wanted.to_string()))?;
    cpal::host_from_id(id).map_err(|e| AudioError::HostUnavailable(e.to_string()))
}

/// The output device `settings` names (or the default device) is present
/// right now. Cheap enough to poll every few seconds while waiting for a
/// device to come back.
pub fn device_present(settings: &AudioSettings) -> bool {
    let Ok(host) = resolve_host(settings.host) else {
        return false;
    };
    match settings.device.as_deref() {
        None => host.default_output_device().is_some(),
        Some(name) => host
            .output_devices()
            .is_ok_and(|mut devices| devices.any(|d| device_name(&d).as_deref() == Some(name))),
    }
}

/// An input device for `settings` is present right now (see
/// [`AudioSettings::input_device`]; without a name the output device's input
/// or the default input counts).
pub fn input_device_present(settings: &AudioSettings) -> bool {
    let Ok(host) = resolve_host(settings.host) else {
        return false;
    };
    select_input_device(
        &host,
        settings.input_device.as_deref(),
        settings.device.as_deref(),
    )
    .is_ok()
}

// ----- Stream -----

const STATUS_LOST: u32 = 1;
const STATUS_ERROR: u32 = 2;
const STATUS_REROUTED: u32 = 4;

#[derive(Default)]
struct StreamStatus {
    flags: AtomicU32,
    /// Latest callback-to-device latency in microseconds, 0 until measured.
    latency_us: AtomicU32,
}

impl StreamStatus {
    fn record(&self, kind: ErrorKind, xruns: &AtomicU32) {
        match kind {
            ErrorKind::Xrun => {
                xruns.fetch_add(1, Ordering::Relaxed);
            }
            ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated => {
                self.flags.fetch_or(STATUS_LOST, Ordering::Release);
            }
            ErrorKind::DeviceChanged => {
                self.flags.fetch_or(STATUS_REROUTED, Ordering::Release);
            }
            _ => {
                self.flags.fetch_or(STATUS_ERROR, Ordering::Release);
            }
        }
    }

    /// Real-time safe: one relaxed store.
    fn set_latency(&self, latency: Duration) {
        let us = u32::try_from(latency.as_micros()).unwrap_or(u32::MAX);
        self.latency_us.store(us, Ordering::Relaxed);
    }

    fn latency(&self) -> Option<Duration> {
        match self.latency_us.load(Ordering::Relaxed) {
            0 => None,
            us => Some(Duration::from_micros(u64::from(us))),
        }
    }

    fn lost(&self) -> bool {
        self.flags.load(Ordering::Acquire) & (STATUS_LOST | STATUS_ERROR) != 0
    }
}

/// What the opened input stream looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputInfo {
    /// Input device name.
    pub device: String,
    /// Device channel count.
    pub channels: u16,
    /// Requested buffer size in frames, `None` for the device default.
    pub buffer_frames: Option<u32>,
    /// Mic channels in use.
    pub mic: Option<InputChannels>,
    /// Aux pair in use.
    pub aux: Option<ChannelPair>,
    /// Frames the engine queues before it consumes input; adds this much
    /// latency to the mic.
    pub prefill_frames: u32,
}

/// What the opened stream looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    /// Host id (`"wasapi"`, `"alsa"`, ...).
    pub host_id: String,
    /// Device name.
    pub device: String,
    /// Sample rate the stream runs at (also the engine rate).
    pub sample_rate: SampleRate,
    /// Device channel count.
    pub channels: u16,
    /// Requested buffer size in frames, `None` for the device default.
    pub buffer_frames: Option<u32>,
    /// Master output pair in use.
    pub master: ChannelPair,
    /// Master is summed to mono.
    pub master_mono: bool,
    /// Headphone pair in use, `None` when disabled or unavailable.
    pub headphones: Option<ChannelPair>,
    /// Headphones are summed to mono.
    pub headphones_mono: bool,
    /// Booth pair in use, `None` when disabled or unavailable.
    pub booth: Option<ChannelPair>,
    /// Booth is summed to mono.
    pub booth_mono: bool,
    /// The device could not carry the requested routing; buses were dropped
    /// or stereo is used.
    pub stereo_fallback: bool,
    /// The input stream, if one is open.
    pub input: Option<InputInfo>,
    /// Why mic and aux are not available although routed, if so.
    pub input_error: Option<String>,
}

/// Gives the input ring's writer back when the stream that owns it is
/// dropped, so a failed or lost input stream can be replaced.
struct ReturnOnDrop {
    producer: Option<InputProducer>,
    back: Sender<InputProducer>,
}

impl Drop for ReturnOnDrop {
    fn drop(&mut self) {
        if let Some(producer) = self.producer.take() {
            // The receiver outlives the streams; if it is gone so is the engine.
            let _ = self.back.send(producer);
        }
    }
}

/// The input half of an [`AudioOutput`].
struct InputPort {
    stream: Option<cpal::Stream>,
    status: Arc<StreamStatus>,
    parked: Option<InputProducer>,
    back_tx: Sender<InputProducer>,
    back_rx: Receiver<InputProducer>,
    shared: Arc<InputShared>,
}

impl InputPort {
    /// The writer, once the stream that held it is dropped.
    fn take_producer(&mut self) -> Option<InputProducer> {
        self.stream = None;
        if let Some(p) = self.parked.take() {
            return Some(p);
        }
        self.back_rx.recv_timeout(Duration::from_millis(500)).ok()
    }
}

/// An open output stream, and the input stream when mic or aux is routed.
/// Keep it alive for as long as audio should play; dropping it stops the
/// streams. Not `Send` on every platform: keep it on the thread that created
/// it.
pub struct AudioOutput {
    _stream: cpal::Stream,
    info: StreamInfo,
    status: Arc<StreamStatus>,
    xruns: Arc<AtomicU32>,
    input: InputPort,
}

impl AudioOutput {
    /// What was opened.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Buffer underruns reported by the backend so far (output and input).
    pub fn xruns(&self) -> u32 {
        self.xruns.load(Ordering::Relaxed)
    }

    /// Latency from the output callback to the device, measured from the
    /// backend's callback timestamps. `None` until the first callback or on a
    /// backend that reports none; fall back to the buffer size.
    pub fn output_latency(&self) -> Option<Duration> {
        self.status.latency()
    }

    /// Latency from the capture to the input callback, measured the same
    /// way. `None` with no input stream. The mic path also adds
    /// [`InputInfo::prefill_frames`] and the output latency.
    pub fn input_latency(&self) -> Option<Duration> {
        self.info.input.as_ref().and(self.input.status.latency())
    }

    /// The input stream died (device unplugged or backend error) while the
    /// output keeps playing. [`reopen_input`](Self::reopen_input) replaces it.
    pub fn input_lost(&self) -> bool {
        self.info.input.is_some() && self.input.status.lost()
    }

    /// Next backend notification, if any: [`EngineEvent::DeviceLost`],
    /// [`EngineEvent::DeviceChanged`] or [`EngineEvent::StreamError`]. Poll
    /// from the control thread next to [`EngineHandle::poll_event`]; these
    /// never travel on the audio thread's event ring. The input stream
    /// reports through [`input_lost`](Self::input_lost) instead, so an
    /// unplugged mic does not look like a lost output.
    pub fn poll_event(&mut self) -> Option<EngineEvent> {
        poll_status(&self.status)
    }

    /// (Re)opens the input stream for the mic and aux routing in `settings`,
    /// dropping the old stream first. The engine keeps running; the input is
    /// silent for the moment it takes. Updates [`info`](Self::info).
    pub fn reopen_input(&mut self, settings: &AudioSettings) -> Result<(), AudioError> {
        let result = self.open_input_inner(settings);
        match &result {
            Ok(info) => {
                self.info.input = Some(info.clone());
                self.info.input_error = None;
            }
            Err(error) => {
                self.info.input = None;
                self.info.input_error = Some(error.to_string());
            }
        }
        result.map(|_| ())
    }

    fn open_input_inner(&mut self, settings: &AudioSettings) -> Result<InputInfo, AudioError> {
        let producer = self.input.take_producer().ok_or(AudioError::InputBusy)?;
        self.input.status = Arc::new(StreamStatus::default());
        if !settings.wants_input() {
            self.input.parked = Some(producer);
            return Err(AudioError::InputChannel);
        }
        let host = resolve_host(settings.host)?;
        let opened = open_input_stream(
            &host,
            settings,
            &self.info.device,
            self.info.sample_rate.0,
            self.info.buffer_frames,
            producer,
            &mut self.input,
            &self.xruns,
        );
        match opened {
            Ok((stream, info)) => {
                self.input.stream = Some(stream);
                Ok(info)
            }
            Err(error) => Err(error),
        }
    }
}

/// Takes the most severe pending backend notification off `status`.
fn poll_status(status: &StreamStatus) -> Option<EngineEvent> {
    for (flag, event) in [
        (STATUS_LOST, EngineEvent::DeviceLost),
        (STATUS_REROUTED, EngineEvent::DeviceChanged),
        (STATUS_ERROR, EngineEvent::StreamError),
    ] {
        if status.flags.fetch_and(!flag, Ordering::AcqRel) & flag != 0 {
            return Some(event);
        }
    }
    None
}

/// Frames to queue before the engine reads input: the larger of the input and
/// output buffers (unknown sizes count as 1024), within the ring.
fn prefill_frames(output_buffer: Option<u32>, input_buffer: Option<u32>) -> u32 {
    let guess = |b: Option<u32>| b.unwrap_or(1024);
    let cap = u32::try_from(INPUT_RING_FRAMES / 4).unwrap_or(u32::MAX);
    guess(output_buffer).max(guess(input_buffer)).clamp(64, cap)
}

/// Opens the output stream described by `settings`, the input stream when a
/// mic or aux is routed, and starts the engine.
///
/// `engine` supplies everything except the sample rate, which comes from the
/// device. Returns the streams and the control-side [`EngineHandle`]. Routing
/// falls back to fewer buses, then to stereo on channels 1/2, when the device
/// cannot carry the requested pairs; see [`StreamInfo::stereo_fallback`]. A
/// failing input does not fail the call: see [`StreamInfo::input_error`].
pub fn open_output(
    settings: &AudioSettings,
    engine: EngineConfig,
) -> Result<(AudioOutput, EngineHandle), AudioError> {
    let host = resolve_host(settings.host)?;
    let device = select_device(&host, settings.device.as_deref())?;
    let ranges = ranges_of(&device, false)?;
    let default_rate = device.default_output_config().ok().map(|c| c.sample_rate());
    let choice = negotiate(&ranges, default_rate, settings).ok_or(AudioError::NoConfig)?;

    let mut buffer_options = Vec::with_capacity(2);
    if let Some(frames) = choice.buffer_frames {
        buffer_options.push(BufferSize::Fixed(frames));
    }
    buffer_options.push(BufferSize::Default);

    let mut last_error = String::new();
    for buffer_size in buffer_options {
        let config = EngineConfig {
            sample_rate: SampleRate(choice.rate),
            max_block_frames: match buffer_size {
                BufferSize::Fixed(n) => engine.max_block_frames.max(n),
                BufferSize::Default => engine.max_block_frames,
            },
            ..engine
        };
        let xruns = Arc::new(AtomicU32::new(0));
        let status = Arc::new(StreamStatus::default());
        let (core, mut handle) = engine_with_xruns(config, Arc::clone(&xruns))?;
        let stream_config = StreamConfig {
            channels: choice.channels,
            sample_rate: choice.rate,
            buffer_size,
        };
        let router = Router::new(
            choice.channels,
            choice.routing,
            config.max_block_frames as usize,
        );
        match build_stream(
            &device,
            choice.format,
            stream_config,
            core,
            router,
            Arc::clone(&status),
            Arc::clone(&xruns),
        ) {
            Ok(stream) => {
                let fixed = match buffer_size {
                    BufferSize::Fixed(n) => Some(n),
                    BufferSize::Default => None,
                };
                let device_label = device_name(&device).unwrap_or_default();
                let (back_tx, back_rx) = channel();
                let mut input = InputPort {
                    stream: None,
                    status: Arc::new(StreamStatus::default()),
                    parked: handle.take_input(),
                    back_tx,
                    back_rx,
                    shared: handle.input_shared(),
                };
                let mut input_info = None;
                let mut input_error = None;
                if settings.wants_input()
                    && let Some(producer) = input.parked.take()
                {
                    match open_input_stream(
                        &host,
                        settings,
                        &device_label,
                        choice.rate,
                        fixed,
                        producer,
                        &mut input,
                        &xruns,
                    ) {
                        Ok((input_stream, info)) => {
                            input.stream = Some(input_stream);
                            input_info = Some(info);
                        }
                        Err(error) => input_error = Some(error.to_string()),
                    }
                }
                stream.play().map_err(|e| AudioError::Play(e.to_string()))?;
                let routing = choice.routing;
                let info = StreamInfo {
                    host_id: host.id().to_string(),
                    device: device_label,
                    sample_rate: SampleRate(choice.rate),
                    channels: choice.channels,
                    buffer_frames: fixed,
                    master: routing.master.pair,
                    master_mono: routing.master.mono,
                    headphones: routing.headphones.map(|r| r.pair),
                    headphones_mono: routing.headphones.is_some_and(|r| r.mono),
                    booth: routing.booth.map(|r| r.pair),
                    booth_mono: routing.booth.is_some_and(|r| r.mono),
                    stereo_fallback: choice.stereo_fallback,
                    input: input_info,
                    input_error,
                };
                let output = AudioOutput {
                    _stream: stream,
                    info,
                    status,
                    xruns,
                    input,
                };
                return Ok((output, handle));
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(AudioError::Build(last_error))
}

fn select_device(host: &Host, name: Option<&str>) -> Result<cpal::Device, AudioError> {
    match name {
        None => host.default_output_device().ok_or(AudioError::NoDevice),
        Some(name) => host
            .output_devices()
            .map_err(|e| AudioError::Devices(e.to_string()))?
            .find(|d| device_name(d).as_deref() == Some(name))
            .ok_or_else(|| AudioError::DeviceNotFound(name.to_string())),
    }
}

/// The named input device; without a name the input of the output device
/// (same name) when it has one, else the host's default input.
fn select_input_device(
    host: &Host,
    name: Option<&str>,
    output_name: Option<&str>,
) -> Result<cpal::Device, AudioError> {
    let find = |wanted: &str| -> Result<Option<cpal::Device>, AudioError> {
        Ok(host
            .input_devices()
            .map_err(|e| AudioError::Devices(e.to_string()))?
            .find(|d| device_name(d).as_deref() == Some(wanted)))
    };
    if let Some(name) = name {
        return find(name)?.ok_or_else(|| AudioError::InputNotFound(name.to_string()));
    }
    if let Some(same) = output_name.map(find).transpose()?.flatten() {
        return Ok(same);
    }
    host.default_input_device().ok_or(AudioError::NoInputDevice)
}

#[allow(clippy::too_many_arguments)]
fn open_input_stream(
    host: &Host,
    settings: &AudioSettings,
    output_name: &str,
    rate: u32,
    output_buffer: Option<u32>,
    producer: InputProducer,
    port: &mut InputPort,
    xruns: &Arc<AtomicU32>,
) -> Result<(cpal::Stream, InputInfo), AudioError> {
    let fail = |port: &mut InputPort, producer: InputProducer, error: AudioError| {
        port.parked = Some(producer);
        error
    };
    let device = match select_input_device(
        host,
        settings.input_device.as_deref(),
        settings.input_device.is_none().then_some(output_name),
    ) {
        Ok(device) => device,
        Err(error) => return Err(fail(port, producer, error)),
    };
    let ranges = match ranges_of(&device, true) {
        Ok(ranges) => ranges,
        Err(error) => return Err(fail(port, producer, error)),
    };
    let choice = match negotiate_input(&ranges, rate, settings) {
        Ok(choice) => choice,
        Err(InputMismatch::ChannelOutOfRange) => {
            return Err(fail(port, producer, AudioError::InputChannel));
        }
        Err(InputMismatch::Unsupported { channels_needed }) => {
            return Err(fail(
                port,
                producer,
                AudioError::InputConfig {
                    channels: channels_needed,
                    rate,
                },
            ));
        }
    };
    let prefill = prefill_frames(output_buffer, choice.buffer_frames);
    port.shared.set_prefill(prefill);
    let router = InputRouter::new(choice.channels, settings.mic, settings.aux);
    let buffer_options = choice
        .buffer_frames
        .map(BufferSize::Fixed)
        .into_iter()
        .chain([BufferSize::Default]);
    let mut producer = Some(producer);
    let mut last_error = String::new();
    for buffer_size in buffer_options {
        let Some(owned) = producer.take() else { break };
        let status = Arc::clone(&port.status);
        let stream_config = StreamConfig {
            channels: choice.channels,
            sample_rate: rate,
            buffer_size,
        };
        let returning = ReturnOnDrop {
            producer: Some(owned),
            back: port.back_tx.clone(),
        };
        match build_input_stream(
            &device,
            choice,
            stream_config,
            router,
            returning,
            status,
            Arc::clone(xruns),
        ) {
            Ok(stream) => match stream.play() {
                Ok(()) => {
                    let info = InputInfo {
                        device: device_name(&device).unwrap_or_default(),
                        channels: choice.channels,
                        buffer_frames: match buffer_size {
                            BufferSize::Fixed(n) => Some(n),
                            BufferSize::Default => None,
                        },
                        mic: settings.mic,
                        aux: settings.aux,
                        prefill_frames: prefill,
                    };
                    return Ok((stream, info));
                }
                Err(error) => {
                    last_error = error.to_string();
                    drop(stream);
                    producer = port.back_rx.recv_timeout(Duration::from_millis(500)).ok();
                }
            },
            Err(error) => {
                last_error = error.to_string();
                // The failed build dropped the closure, which sent the writer back.
                producer = port.back_rx.try_recv().ok();
            }
        }
    }
    if let Some(p) = producer {
        port.parked = Some(p);
    }
    Err(AudioError::InputBuild(last_error))
}

fn build_input_stream(
    device: &cpal::Device,
    choice: InputChoice,
    config: StreamConfig,
    router: InputRouter,
    returning: ReturnOnDrop,
    status: Arc<StreamStatus>,
    xruns: Arc<AtomicU32>,
) -> Result<cpal::Stream, cpal::Error> {
    match choice.format {
        SampleFormat::F32 => {
            build_input_typed::<f32>(device, config, router, returning, status, xruns)
        }
        SampleFormat::I32 => {
            build_input_typed::<i32>(device, config, router, returning, status, xruns)
        }
        SampleFormat::I16 => {
            build_input_typed::<i16>(device, config, router, returning, status, xruns)
        }
        _ => Err(cpal::Error::new(ErrorKind::UnsupportedConfig)),
    }
}

fn build_input_typed<T>(
    device: &cpal::Device,
    config: StreamConfig,
    router: InputRouter,
    mut returning: ReturnOnDrop,
    status: Arc<StreamStatus>,
    xruns: Arc<AtomicU32>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let errors = Arc::clone(&status);
    device.build_input_stream(
        config,
        move |data: &[T], info| {
            let stamp = info.timestamp();
            status.set_latency(stamp.callback.duration_since(stamp.capture));
            if let Some(producer) = returning.producer.as_mut() {
                router.process(data, producer);
            }
        },
        move |error| errors.record(error.kind(), &xruns),
        None,
    )
}

fn build_stream(
    device: &cpal::Device,
    format: SampleFormat,
    config: StreamConfig,
    core: EngineCore,
    router: Router,
    status: Arc<StreamStatus>,
    xruns: Arc<AtomicU32>,
) -> Result<cpal::Stream, cpal::Error> {
    match format {
        SampleFormat::F32 => build_typed::<f32>(device, config, core, router, status, xruns),
        SampleFormat::I32 => build_typed::<i32>(device, config, core, router, status, xruns),
        SampleFormat::I16 => build_typed::<i16>(device, config, core, router, status, xruns),
        _ => Err(cpal::Error::new(ErrorKind::UnsupportedConfig)),
    }
}

fn build_typed<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut core: EngineCore,
    mut router: Router,
    status: Arc<StreamStatus>,
    xruns: Arc<AtomicU32>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let errors = Arc::clone(&status);
    device.build_output_stream(
        config,
        move |data: &mut [T], info| {
            let stamp = info.timestamp();
            status.set_latency(stamp.playback.duration_since(stamp.callback));
            router.render(&mut core, data);
        },
        move |error| errors.record(error.kind(), &xruns),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_maps_backend_errors() {
        let status = StreamStatus::default();
        let xruns = AtomicU32::new(0);
        status.record(ErrorKind::Xrun, &xruns);
        assert_eq!(xruns.load(Ordering::Relaxed), 1);
        status.record(ErrorKind::DeviceNotAvailable, &xruns);
        assert_eq!(status.flags.load(Ordering::Relaxed), STATUS_LOST);
        assert!(status.lost());
    }

    #[test]
    fn latency_is_recorded_from_timestamps() {
        let status = StreamStatus::default();
        assert_eq!(status.latency(), None);
        status.set_latency(Duration::from_micros(10_667));
        assert_eq!(status.latency(), Some(Duration::from_micros(10_667)));
    }

    #[test]
    fn backend_errors_surface_as_engine_events() {
        let status = Arc::new(StreamStatus::default());
        let xruns = AtomicU32::new(0);
        status.record(ErrorKind::DeviceChanged, &xruns);
        status.record(ErrorKind::DeviceNotAvailable, &xruns);
        status.record(ErrorKind::ResourceExhausted, &xruns);
        let mut seen = [None; 4];
        for slot in &mut seen {
            *slot = poll_status(&status);
        }
        assert_eq!(
            seen,
            [
                Some(EngineEvent::DeviceLost),
                Some(EngineEvent::DeviceChanged),
                Some(EngineEvent::StreamError),
                None,
            ]
        );
    }

    #[test]
    fn prefill_follows_the_larger_buffer_within_the_ring() {
        assert_eq!(prefill_frames(Some(256), Some(512)), 512);
        assert_eq!(prefill_frames(Some(2048), None), 2048);
        assert_eq!(prefill_frames(None, None), 1024);
        assert_eq!(prefill_frames(Some(8), Some(8)), 64);
        assert_eq!(prefill_frames(Some(60_000), None), 2048);
    }

    #[test]
    fn the_input_writer_comes_back_when_its_stream_is_dropped() {
        let (_core, mut handle) = crate::engine(EngineConfig::default()).expect("engine");
        let producer = handle.take_input().expect("producer");
        let (tx, rx) = channel();
        let returning = ReturnOnDrop {
            producer: Some(producer),
            back: tx,
        };
        assert!(rx.try_recv().is_err());
        drop(returning);
        assert!(rx.try_recv().is_ok(), "writer returned for the next stream");
    }

    #[test]
    fn listing_hosts_and_devices_never_panics_without_hardware() {
        let hosts = list_hosts();
        println!("hosts: {hosts:?}");
        for list in [list_output_devices(None), list_input_devices(None)] {
            match list {
                Ok(devices) => println!("devices: {devices:?}"),
                Err(error) => println!("no device listing here: {error}"),
            }
        }
    }

    #[test]
    fn unknown_device_name_is_an_error_not_a_panic() {
        let settings = AudioSettings {
            device: Some("opendeck-no-such-device".to_string()),
            ..AudioSettings::default()
        };
        assert!(!device_present(&settings));
        match open_output(&settings, EngineConfig::default()) {
            Ok(_) => panic!("a device with that name cannot exist"),
            Err(error) => println!("expected failure: {error}"),
        }
    }

    #[test]
    fn default_device_opens_or_the_test_skips() {
        let settings = AudioSettings::default();
        match open_output(&settings, EngineConfig::default()) {
            Ok((mut output, mut handle)) => {
                println!("opened {:?}", output.info());
                std::thread::sleep(Duration::from_millis(150));
                assert!(output.poll_event().is_none());
                assert!(handle.poll_telemetry().is_some(), "telemetry flows");
                println!("latency {:?}", output.output_latency());
            }
            Err(error) => println!("skipped, no usable output device: {error}"),
        }
    }

    #[test]
    fn alsa_null_sink_plays_through_four_channel_routing_or_the_test_skips() {
        let Ok(devices) = list_output_devices(None) else {
            println!("skipped: no device listing");
            return;
        };
        let Some(null) = devices
            .iter()
            .find(|d| d.name.contains("Discard all samples") && d.max_channels >= 4)
        else {
            println!("skipped: no null sink on this host");
            return;
        };
        let settings = AudioSettings {
            device: Some(null.name.clone()),
            ..AudioSettings::default()
        };
        let (output, mut handle) = match open_output(&settings, EngineConfig::default()) {
            Ok(opened) => opened,
            Err(error) => {
                println!("skipped: could not open the null sink: {error}");
                return;
            }
        };
        let info = output.info();
        println!("opened {info:?}");
        assert_eq!(info.channels, 4);
        assert_eq!(info.master, ChannelPair { left: 0, right: 1 });
        assert_eq!(info.headphones, Some(ChannelPair { left: 2, right: 3 }));
        assert_eq!(info.booth, None);
        assert!(!info.stereo_fallback);

        // The ALSA null plugin is not a clock: how long the callback keeps
        // running varies, so only require that it ran at least once.
        std::thread::sleep(Duration::from_millis(200));
        let telemetry = handle
            .poll_telemetry()
            .expect("telemetry from the callback");
        assert_eq!(telemetry.sample_rate, SampleRate(48_000));
    }
}
