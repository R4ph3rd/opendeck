//! Audio device layer on cpal 0.18: list hosts and output devices, open an
//! output stream around an [`EngineCore`], route the engine's buses to the
//! device channels.
//!
//! Nothing here runs on the audio thread except the stream callback, which
//! renders the core and copies the master bus to its channel pair. Errors
//! reported by the backend (device unplugged, xruns) arrive on a backend
//! thread and are published through atomics; [`AudioOutput::poll_event`]
//! turns them into device [`EngineEvent`]s for the control thread.
//!
//! The `asio` cargo feature forwards to `cpal/asio` (Windows only).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, ErrorKind, FromSample, Host, HostId, Sample, SampleFormat, SizedSample,
    StreamConfig, SupportedBufferSize,
};
use opendeck_contracts::settings::{AudioHostKind, AudioSettings, ChannelPair};
use opendeck_contracts::{EngineEvent, SampleRate};

use crate::audio::EngineCore;
use crate::handle::{EngineHandle, engine_with_xruns};
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
}

/// An output device and what it supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDeviceInfo {
    /// Host id this device belongs to.
    pub host_id: String,
    /// Device name as the host reports it. Used to select the device.
    pub name: String,
    /// The host's default output device.
    pub is_default: bool,
    /// Most output channels any configuration offers.
    pub max_channels: u16,
    /// Default sample rate in Hz, if the host reports one.
    pub default_sample_rate: Option<u32>,
    /// Supported sample rate ranges `(min, max)` in Hz, deduplicated.
    pub sample_rate_ranges: Vec<(u32, u32)>,
    /// Supported buffer size range `(min, max)` in frames, if known.
    pub buffer_frames: Option<(u32, u32)>,
}

/// Lists the hosts compiled in and running on this machine.
pub fn list_hosts() -> Vec<HostInfo> {
    let default = cpal::default_host().id();
    cpal::available_hosts()
        .into_iter()
        .map(|id| HostInfo {
            id: id.to_string(),
            name: id.name().to_string(),
            kind: host_kind(id),
            is_default: id == default,
        })
        .collect()
}

/// Lists output devices of `host` (`None` for the default host).
pub fn list_output_devices(
    host: Option<AudioHostKind>,
) -> Result<Vec<OutputDeviceInfo>, AudioError> {
    let host = resolve_host(host)?;
    let default_name = host.default_output_device().and_then(|d| device_name(&d));
    let devices = host
        .output_devices()
        .map_err(|e| AudioError::Devices(e.to_string()))?;
    let host_id = host.id().to_string();
    Ok(devices
        .filter_map(|device| {
            let name = device_name(&device)?;
            let ranges = device.supported_output_configs().ok()?.collect::<Vec<_>>();
            let mut sample_rate_ranges: Vec<(u32, u32)> = ranges
                .iter()
                .map(|r| (r.min_sample_rate(), r.max_sample_rate()))
                .collect();
            sample_rate_ranges.sort_unstable();
            sample_rate_ranges.dedup();
            let buffer_frames = ranges
                .iter()
                .filter_map(|r| match r.buffer_size() {
                    SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                    SupportedBufferSize::Unknown => None,
                })
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
            Some(OutputDeviceInfo {
                host_id: host_id.clone(),
                is_default: default_name.as_deref() == Some(name.as_str()),
                max_channels: ranges.iter().map(|r| r.channels()).max().unwrap_or(0),
                default_sample_rate: device.default_output_config().ok().map(|c| c.sample_rate()),
                sample_rate_ranges,
                buffer_frames,
                name,
            })
        })
        .collect())
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

// ----- Negotiation (pure, tested without a device) -----

/// One supported configuration range, reduced to what negotiation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RangeInfo {
    pub(crate) channels: u16,
    pub(crate) min_rate: u32,
    pub(crate) max_rate: u32,
    pub(crate) buffer: Option<(u32, u32)>,
    pub(crate) format: SampleFormat,
}

/// The configuration chosen for the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) channels: u16,
    pub(crate) rate: u32,
    pub(crate) format: SampleFormat,
    pub(crate) buffer_frames: Option<u32>,
    pub(crate) master: ChannelPair,
    pub(crate) headphones: Option<ChannelPair>,
    /// The device cannot carry the requested routing; fell back to stereo.
    pub(crate) stereo_fallback: bool,
}

fn format_rank(format: SampleFormat) -> Option<u8> {
    match format {
        SampleFormat::F32 => Some(0),
        SampleFormat::I32 => Some(1),
        SampleFormat::I16 => Some(2),
        _ => None,
    }
}

fn pair_max(pair: ChannelPair) -> u16 {
    pair.left.max(pair.right)
}

/// Picks channels, rate, format and buffer size for `settings` from what the
/// device offers. Tries the full routing first, then master only, then plain
/// stereo on channels 1/2.
pub(crate) fn negotiate(
    ranges: &[RangeInfo],
    default_rate: Option<u32>,
    settings: &AudioSettings,
) -> Option<Choice> {
    let want_rate = settings.sample_rate.or(default_rate).unwrap_or(48_000);
    let stereo = ChannelPair { left: 0, right: 1 };
    let mut tiers: [Option<(u16, ChannelPair, Option<ChannelPair>)>; 3] = [None; 3];
    let mut tier_count = 0;
    if let Some(hp) = settings.headphones {
        let need = pair_max(settings.master)
            .max(pair_max(hp))
            .saturating_add(1);
        tiers[tier_count] = Some((need, settings.master, Some(hp)));
        tier_count += 1;
    }
    tiers[tier_count] = Some((
        pair_max(settings.master).saturating_add(1),
        settings.master,
        None,
    ));
    tier_count += 1;
    tiers[tier_count] = Some((2, stereo, None));

    for (tier, entry) in tiers.iter().flatten().enumerate() {
        let (need, master, headphones) = *entry;
        let usable = |r: &&RangeInfo| r.channels >= need && format_rank(r.format).is_some();
        let nearest = |r: &RangeInfo| want_rate.clamp(r.min_rate, r.max_rate);
        let Some(rate) = ranges
            .iter()
            .filter(usable)
            .map(nearest)
            .min_by_key(|rate| rate.abs_diff(want_rate))
        else {
            continue;
        };
        let best = ranges
            .iter()
            .filter(usable)
            .filter(|r| (r.min_rate..=r.max_rate).contains(&rate))
            .min_by_key(|r| (r.channels, format_rank(r.format)))?;
        let buffer_frames = settings.buffer_frames.map(|n| {
            best.buffer
                .map_or(n, |(min, max)| n.clamp(min, max.max(min)))
        });
        let asked_for_headphones = settings.headphones.is_some();
        return Some(Choice {
            channels: best.channels,
            rate,
            format: best.format,
            buffer_frames,
            master,
            headphones,
            stereo_fallback: tier > 0 || (asked_for_headphones && headphones.is_none()),
        });
    }
    None
}

// ----- Routing -----

/// Copies the engine's stereo master into the master channel pair of an
/// interleaved device buffer and silences every other channel. The
/// headphone pair stays silent until the cue bus exists (M2).
pub(crate) struct Router {
    channels: usize,
    master: (usize, usize),
    scratch: Vec<f32>,
    max_block: usize,
}

impl Router {
    pub(crate) fn new(channels: u16, master: ChannelPair, max_block_frames: usize) -> Self {
        Self {
            channels: usize::from(channels).max(1),
            master: (usize::from(master.left), usize::from(master.right)),
            scratch: vec![0.0; max_block_frames.max(1) * 2],
            max_block: max_block_frames.max(1),
        }
    }

    /// Renders `core` into the interleaved device buffer `out`.
    pub(crate) fn render<T: Sample + FromSample<f32>>(
        &mut self,
        core: &mut EngineCore,
        out: &mut [T],
    ) {
        for block in out.chunks_mut(self.channels * self.max_block) {
            let frames = block.len() / self.channels;
            let Some(stereo) = self.scratch.get_mut(..frames * 2) else {
                block.fill(T::EQUILIBRIUM);
                continue;
            };
            core.render(stereo);
            route(stereo, block, self.channels, self.master);
        }
    }
}

/// Writes interleaved stereo `src` into `dst` (`channels` per frame): the
/// master pair gets the signal, everything else is silent. Pair indexes
/// beyond `channels` are ignored.
pub(crate) fn route<T: Sample + FromSample<f32>>(
    src: &[f32],
    dst: &mut [T],
    channels: usize,
    master: (usize, usize),
) {
    for (frame, stereo) in dst
        .chunks_exact_mut(channels.max(1))
        .zip(src.chunks_exact(2))
    {
        frame.fill(T::EQUILIBRIUM);
        if let Some(slot) = frame.get_mut(master.0) {
            *slot = T::from_sample(stereo[0]);
        }
        if let Some(slot) = frame.get_mut(master.1) {
            *slot = T::from_sample(stereo[1]);
        }
    }
    // A trailing partial frame (never produced by hosts) stays silent.
    let tail = dst.len() - dst.len() % channels.max(1);
    if let Some(rest) = dst.get_mut(tail..) {
        rest.fill(T::EQUILIBRIUM);
    }
}

// ----- Stream -----

const STATUS_LOST: u32 = 1;
const STATUS_ERROR: u32 = 2;
const STATUS_REROUTED: u32 = 4;

#[derive(Default)]
struct StreamStatus {
    flags: AtomicU32,
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
    /// Headphone pair in use, `None` when disabled or unavailable.
    pub headphones: Option<ChannelPair>,
    /// The device could not carry the requested routing; stereo is used.
    pub stereo_fallback: bool,
}

/// An open output stream. Keep it alive for as long as audio should play;
/// dropping it stops the stream. Not `Send` on every platform: keep it on
/// the thread that created it.
pub struct AudioOutput {
    _stream: cpal::Stream,
    info: StreamInfo,
    status: Arc<StreamStatus>,
    xruns: Arc<AtomicU32>,
}

impl AudioOutput {
    /// What was opened.
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    /// Buffer underruns reported by the backend so far.
    pub fn xruns(&self) -> u32 {
        self.xruns.load(Ordering::Relaxed)
    }

    /// Next backend notification, if any: [`EngineEvent::DeviceLost`],
    /// [`EngineEvent::DeviceChanged`] or [`EngineEvent::StreamError`]. Poll
    /// from the control thread next to [`EngineHandle::poll_event`]; these
    /// never travel on the audio thread's event ring.
    pub fn poll_event(&mut self) -> Option<EngineEvent> {
        poll_status(&self.status)
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

/// Opens the output stream described by `settings` and starts the engine.
///
/// `engine` supplies everything except the sample rate, which comes from the
/// device. Returns the stream and the control-side [`EngineHandle`]. Routing
/// falls back to stereo on channels 1/2 when the device cannot carry the
/// requested pairs; see [`StreamInfo::stereo_fallback`].
pub fn open_output(
    settings: &AudioSettings,
    engine: EngineConfig,
) -> Result<(AudioOutput, EngineHandle), AudioError> {
    let host = resolve_host(settings.host)?;
    let device = select_device(&host, settings.device.as_deref())?;
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| AudioError::Devices(e.to_string()))?
        .map(|r| RangeInfo {
            channels: r.channels(),
            min_rate: r.min_sample_rate(),
            max_rate: r.max_sample_rate(),
            buffer: match r.buffer_size() {
                SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                SupportedBufferSize::Unknown => None,
            },
            format: r.sample_format(),
        })
        .collect();
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
        let (core, handle) = engine_with_xruns(config, Arc::clone(&xruns))?;
        let stream_config = StreamConfig {
            channels: choice.channels,
            sample_rate: choice.rate,
            buffer_size,
        };
        let router = Router::new(
            choice.channels,
            choice.master,
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
                stream.play().map_err(|e| AudioError::Play(e.to_string()))?;
                let info = StreamInfo {
                    host_id: host.id().to_string(),
                    device: device_name(&device).unwrap_or_default(),
                    sample_rate: SampleRate(choice.rate),
                    channels: choice.channels,
                    buffer_frames: match buffer_size {
                        BufferSize::Fixed(n) => Some(n),
                        BufferSize::Default => None,
                    },
                    master: choice.master,
                    headphones: choice.headphones,
                    stereo_fallback: choice.stereo_fallback,
                };
                let output = AudioOutput {
                    _stream: stream,
                    info,
                    status,
                    xruns,
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
    device.build_output_stream(
        config,
        move |data: &mut [T], _| router.render(&mut core, data),
        move |error| status.record(error.kind(), &xruns),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(channels: u16, min: u32, max: u32, format: SampleFormat) -> RangeInfo {
        RangeInfo {
            channels,
            min_rate: min,
            max_rate: max,
            buffer: Some((64, 4096)),
            format,
        }
    }

    #[test]
    fn four_channel_device_gets_master_and_headphone_pairs() {
        let ranges = [
            range(2, 44_100, 96_000, SampleFormat::F32),
            range(4, 44_100, 96_000, SampleFormat::F32),
        ];
        let choice = negotiate(&ranges, Some(48_000), &AudioSettings::default()).expect("choice");
        assert_eq!(choice.channels, 4);
        assert_eq!(choice.rate, 48_000);
        assert_eq!(choice.master, ChannelPair { left: 0, right: 1 });
        assert_eq!(choice.headphones, Some(ChannelPair { left: 2, right: 3 }));
        assert!(!choice.stereo_fallback);
    }

    #[test]
    fn stereo_device_falls_back_without_headphones() {
        let ranges = [range(2, 44_100, 48_000, SampleFormat::F32)];
        let choice = negotiate(&ranges, Some(48_000), &AudioSettings::default()).expect("choice");
        assert_eq!(choice.channels, 2);
        assert_eq!(choice.headphones, None);
        assert!(choice.stereo_fallback);
    }

    #[test]
    fn master_on_high_pair_is_kept_when_it_fits() {
        let settings = AudioSettings {
            master: ChannelPair { left: 4, right: 5 },
            headphones: None,
            ..AudioSettings::default()
        };
        let ranges = [
            range(2, 48_000, 48_000, SampleFormat::F32),
            range(8, 48_000, 48_000, SampleFormat::F32),
        ];
        let choice = negotiate(&ranges, None, &settings).expect("choice");
        assert_eq!(choice.channels, 8);
        assert_eq!(choice.master, ChannelPair { left: 4, right: 5 });
        assert!(!choice.stereo_fallback);

        let stereo_only = [range(2, 48_000, 48_000, SampleFormat::F32)];
        let choice = negotiate(&stereo_only, None, &settings).expect("choice");
        assert_eq!(choice.master, ChannelPair { left: 0, right: 1 });
        assert!(choice.stereo_fallback);
    }

    #[test]
    fn rate_and_buffer_are_clamped_to_the_device() {
        let settings = AudioSettings {
            sample_rate: Some(192_000),
            buffer_frames: Some(16),
            headphones: None,
            ..AudioSettings::default()
        };
        let ranges = [range(2, 44_100, 96_000, SampleFormat::F32)];
        let choice = negotiate(&ranges, None, &settings).expect("choice");
        assert_eq!(choice.rate, 96_000);
        assert_eq!(choice.buffer_frames, Some(64));
    }

    #[test]
    fn prefers_f32_and_rejects_unusable_formats() {
        let ranges = [
            range(2, 48_000, 48_000, SampleFormat::I16),
            range(2, 48_000, 48_000, SampleFormat::F32),
            range(2, 48_000, 48_000, SampleFormat::F64),
        ];
        let choice = negotiate(&ranges, None, &AudioSettings::default()).expect("choice");
        assert_eq!(choice.format, SampleFormat::F32);
        let only_f64 = [range(2, 48_000, 48_000, SampleFormat::F64)];
        assert!(negotiate(&only_f64, None, &AudioSettings::default()).is_none());
        let mono = [range(1, 48_000, 48_000, SampleFormat::F32)];
        assert!(negotiate(&mono, None, &AudioSettings::default()).is_none());
    }

    #[test]
    fn route_places_master_and_silences_the_rest() {
        let src = [0.5, -0.5, 0.25, -0.25];
        let mut dst = [9.0_f32; 8];
        route(&src, &mut dst, 4, (0, 1));
        assert_eq!(dst, [0.5, -0.5, 0.0, 0.0, 0.25, -0.25, 0.0, 0.0]);

        let mut dst = [9.0_f32; 8];
        route(&src, &mut dst, 4, (2, 3));
        assert_eq!(dst, [0.0, 0.0, 0.5, -0.5, 0.0, 0.0, 0.25, -0.25]);

        // Pair outside the device: silence, no panic.
        let mut dst = [9.0_f32; 4];
        route(&src, &mut dst, 2, (6, 7));
        assert_eq!(dst, [0.0; 4]);

        let mut dst = [i16::MAX; 4];
        route(&src, &mut dst, 2, (0, 1));
        assert_eq!(dst[0], i16::from_sample(0.5_f32));
    }

    #[test]
    fn router_renders_the_core_into_a_four_channel_buffer() {
        let (mut core, _handle) = crate::engine(EngineConfig::default()).expect("engine");
        let mut router = Router::new(4, ChannelPair { left: 0, right: 1 }, 64);
        let mut out = vec![1.0_f32; 4 * 200];
        router.render(&mut core, &mut out);
        assert!(out.iter().all(|s| *s == 0.0), "an empty engine is silent");
    }

    #[test]
    fn status_maps_backend_errors() {
        let status = StreamStatus::default();
        let xruns = AtomicU32::new(0);
        status.record(ErrorKind::Xrun, &xruns);
        assert_eq!(xruns.load(Ordering::Relaxed), 1);
        status.record(ErrorKind::DeviceNotAvailable, &xruns);
        assert_eq!(status.flags.load(Ordering::Relaxed), STATUS_LOST);
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
    fn listing_hosts_and_devices_never_panics_without_hardware() {
        let hosts = list_hosts();
        println!("hosts: {hosts:?}");
        match list_output_devices(None) {
            Ok(devices) => println!("devices: {devices:?}"),
            Err(error) => println!("no device listing here: {error}"),
        }
    }

    #[test]
    fn unknown_device_name_is_an_error_not_a_panic() {
        let settings = AudioSettings {
            device: Some("opendeck-no-such-device".to_string()),
            ..AudioSettings::default()
        };
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
                std::thread::sleep(std::time::Duration::from_millis(150));
                assert!(output.poll_event().is_none());
                assert!(handle.poll_telemetry().is_some(), "telemetry flows");
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
        assert!(!info.stereo_fallback);

        // The ALSA null plugin is not a clock: how long the callback keeps
        // running varies, so only require that it ran at least once.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let telemetry = handle
            .poll_telemetry()
            .expect("telemetry from the callback");
        assert_eq!(telemetry.sample_rate, SampleRate(48_000));
    }
}
