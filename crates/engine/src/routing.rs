//! Channel routing between the engine's buses and a device: picks a stream
//! configuration for the requested pairs ([`negotiate`], [`negotiate_input`])
//! and copies buses into the channels of an interleaved device buffer
//! ([`Router`]) or channels of an input buffer into the input ring
//! ([`InputRouter`]). Pure code, tested without a device.
//!
//! Output buses are master, headphones and booth (a copy of the master with
//! its own level). Each goes to any stereo pair of the device (up to
//! [`MAX_DEVICE_CHANNELS`] channels) and can be summed to mono. Pairs that
//! overlap are written in the order master, headphones, booth, so the later
//! bus wins on a shared channel.

use cpal::{FromSample, Sample, SampleFormat};
use opendeck_contracts::settings::{
    AudioSettings, ChannelPair, InputChannels, MAX_DEVICE_CHANNELS,
};

use crate::audio::EngineCore;
use crate::inputs::{INPUT_CHANNELS, InputProducer};

/// One supported configuration range, reduced to what negotiation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RangeInfo {
    pub(crate) channels: u16,
    pub(crate) min_rate: u32,
    pub(crate) max_rate: u32,
    pub(crate) buffer: Option<(u32, u32)>,
    pub(crate) format: SampleFormat,
}

/// Where one output bus goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputRoute {
    pub(crate) pair: ChannelPair,
    /// Play the sum of left and right on both channels.
    pub(crate) mono: bool,
}

/// The output buses a device carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Routing {
    pub(crate) master: OutputRoute,
    pub(crate) headphones: Option<OutputRoute>,
    pub(crate) booth: Option<OutputRoute>,
}

impl Routing {
    /// The routing `settings` asks for, before any device limits.
    pub(crate) fn requested(settings: &AudioSettings) -> Self {
        Self {
            master: OutputRoute {
                pair: settings.master,
                mono: settings.master_mono,
            },
            headphones: settings.headphones.map(|pair| OutputRoute {
                pair,
                mono: settings.headphones_mono,
            }),
            booth: settings.booth.map(|pair| OutputRoute {
                pair,
                mono: settings.booth_mono,
            }),
        }
    }

    /// Channels the device needs to carry every bus, `None` when a pair is
    /// outside [`MAX_DEVICE_CHANNELS`].
    fn channels_needed(&self) -> Option<u16> {
        let pairs = [Some(self.master), self.headphones, self.booth];
        let highest = pairs
            .into_iter()
            .flatten()
            .map(|r| r.pair.left.max(r.pair.right))
            .max()?;
        (highest < MAX_DEVICE_CHANNELS).then_some(highest + 1)
    }
}

/// The output configuration chosen for the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Choice {
    pub(crate) channels: u16,
    pub(crate) rate: u32,
    pub(crate) format: SampleFormat,
    pub(crate) buffer_frames: Option<u32>,
    pub(crate) routing: Routing,
    /// The device cannot carry the requested routing; some buses were
    /// dropped or everything fell back to stereo.
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

/// Smallest usable range that carries `need` channels at `rate`.
fn best_range(ranges: &[RangeInfo], need: u16, rate: u32) -> Option<&RangeInfo> {
    ranges
        .iter()
        .filter(|r| r.channels >= need && format_rank(r.format).is_some())
        .filter(|r| (r.min_rate..=r.max_rate).contains(&rate))
        .min_by_key(|r| (r.channels, format_rank(r.format)))
}

/// Picks channels, rate, format and buffer size for `settings` from what the
/// device offers. Tries the full routing first, then without the booth, then
/// the master alone, then plain stereo on channels 1/2.
pub(crate) fn negotiate(
    ranges: &[RangeInfo],
    default_rate: Option<u32>,
    settings: &AudioSettings,
) -> Option<Choice> {
    let want_rate = settings.sample_rate.or(default_rate).unwrap_or(48_000);
    let requested = Routing::requested(settings);
    let stereo = Routing {
        master: OutputRoute {
            pair: ChannelPair { left: 0, right: 1 },
            mono: settings.master_mono,
        },
        headphones: None,
        booth: None,
    };
    let tiers = [
        requested,
        Routing {
            booth: None,
            ..requested
        },
        Routing {
            headphones: None,
            booth: None,
            ..requested
        },
        stereo,
    ];
    for routing in tiers {
        let Some(need) = routing.channels_needed() else {
            continue;
        };
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
        let best = best_range(ranges, need, rate)?;
        let buffer_frames = settings.buffer_frames.map(|n| {
            best.buffer
                .map_or(n, |(min, max)| n.clamp(min, max.max(min)))
        });
        return Some(Choice {
            channels: best.channels,
            rate,
            format: best.format,
            buffer_frames,
            routing,
            stereo_fallback: routing != requested,
        });
    }
    None
}

/// The input configuration chosen for the input stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputChoice {
    pub(crate) channels: u16,
    pub(crate) format: SampleFormat,
    pub(crate) buffer_frames: Option<u32>,
}

/// Why no input configuration fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputMismatch {
    /// A channel index is outside [`MAX_DEVICE_CHANNELS`].
    ChannelOutOfRange,
    /// No configuration carries the channels at the output rate.
    Unsupported { channels_needed: u16 },
}

/// Highest input channel index plus one over the routed mic and aux.
pub(crate) fn input_channels_needed(
    mic: Option<InputChannels>,
    aux: Option<ChannelPair>,
) -> Option<u16> {
    let mic_high = mic.map(|m| m.left.max(m.right.unwrap_or(0)));
    let aux_high = aux.map(|a| a.left.max(a.right));
    let highest = mic_high.into_iter().chain(aux_high).max()?;
    Some(highest.saturating_add(1))
}

/// Picks an input configuration at `rate` (the output stream's rate: there is
/// no resampler) that carries the routed channels.
pub(crate) fn negotiate_input(
    ranges: &[RangeInfo],
    rate: u32,
    settings: &AudioSettings,
) -> Result<InputChoice, InputMismatch> {
    let need = input_channels_needed(settings.mic, settings.aux)
        .filter(|n| *n <= MAX_DEVICE_CHANNELS)
        .ok_or(InputMismatch::ChannelOutOfRange)?;
    let best = best_range(ranges, need, rate).ok_or(InputMismatch::Unsupported {
        channels_needed: need,
    })?;
    Ok(InputChoice {
        channels: best.channels,
        format: best.format,
        buffer_frames: settings.buffer_frames.map(|n| {
            best.buffer
                .map_or(n, |(min, max)| n.clamp(min, max.max(min)))
        }),
    })
}

// ----- Output routing -----

#[derive(Debug, Clone, Copy)]
struct Slot {
    left: usize,
    right: usize,
    mono: bool,
}

impl From<OutputRoute> for Slot {
    fn from(route: OutputRoute) -> Self {
        Self {
            left: usize::from(route.pair.left),
            right: usize::from(route.pair.right),
            mono: route.mono,
        }
    }
}

/// Copies the engine's master, headphone and booth buses into their channel
/// pairs of an interleaved device buffer. Every other channel is silent.
pub(crate) struct Router {
    channels: usize,
    master: Slot,
    headphones: Option<Slot>,
    booth: Option<Slot>,
    scratch: Vec<f32>,
    headphone_scratch: Vec<f32>,
    booth_scratch: Vec<f32>,
    max_block: usize,
}

impl Router {
    pub(crate) fn new(channels: u16, routing: Routing, max_block_frames: usize) -> Self {
        let max_block = max_block_frames.max(1);
        let buffer = || vec![0.0; max_block * 2];
        Self {
            channels: usize::from(channels).max(1),
            master: routing.master.into(),
            headphones: routing.headphones.map(Slot::from),
            booth: routing.booth.map(Slot::from),
            scratch: buffer(),
            headphone_scratch: buffer(),
            booth_scratch: buffer(),
            max_block,
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
            let (Some(stereo), Some(phones), Some(booth)) = (
                self.scratch.get_mut(..frames * 2),
                self.headphone_scratch.get_mut(..frames * 2),
                self.booth_scratch.get_mut(..frames * 2),
            ) else {
                block.fill(T::EQUILIBRIUM);
                continue;
            };
            core.render_with_booth(stereo, phones, booth);
            block.fill(T::EQUILIBRIUM);
            route_pair(stereo, block, self.channels, self.master);
            if let Some(slot) = self.headphones {
                route_pair(phones, block, self.channels, slot);
            }
            if let Some(slot) = self.booth {
                route_pair(booth, block, self.channels, slot);
            }
        }
    }
}

/// Writes interleaved stereo `src` into one channel pair of `dst`
/// (`channels` per frame), leaving the other channels alone. A channel beyond
/// `channels` is ignored. With `mono` both channels get the mean of left and
/// right.
fn route_pair<T: Sample + FromSample<f32>>(
    src: &[f32],
    dst: &mut [T],
    channels: usize,
    slot: Slot,
) {
    for (frame, stereo) in dst
        .chunks_exact_mut(channels.max(1))
        .zip(src.chunks_exact(2))
    {
        let (l, r) = if slot.mono {
            let sum = (stereo[0] + stereo[1]) * 0.5;
            (sum, sum)
        } else {
            (stereo[0], stereo[1])
        };
        if let Some(out) = frame.get_mut(slot.left) {
            *out = T::from_sample(l);
        }
        if let Some(out) = frame.get_mut(slot.right) {
            *out = T::from_sample(r);
        }
    }
}

// ----- Input routing -----

/// Frames converted per step; keeps the scratch on the stack.
const INPUT_STEP: usize = 256;

/// Picks the mic and aux channels out of an interleaved input buffer and
/// pushes them to the input ring as [`INPUT_CHANNELS`] samples per frame.
/// Real-time safe: no allocation, the scratch is on the stack.
#[derive(Debug, Clone, Copy)]
pub(crate) struct InputRouter {
    channels: usize,
    mic: Option<(usize, usize)>,
    aux: Option<(usize, usize)>,
}

impl InputRouter {
    pub(crate) fn new(channels: u16, mic: Option<InputChannels>, aux: Option<ChannelPair>) -> Self {
        Self {
            channels: usize::from(channels).max(1),
            mic: mic.map(|m| {
                let left = usize::from(m.left);
                (left, m.right.map_or(left, usize::from))
            }),
            aux: aux.map(|a| (usize::from(a.left), usize::from(a.right))),
        }
    }

    /// Converts and queues `data`. Returns the frames the ring refused.
    pub(crate) fn process<T: Sample>(&self, data: &[T], producer: &mut InputProducer) -> usize
    where
        f32: FromSample<T>,
    {
        let mut refused = 0;
        for block in data.chunks(self.channels * INPUT_STEP) {
            let mut scratch = [0.0_f32; INPUT_STEP * INPUT_CHANNELS];
            let mut frames = 0;
            for (frame, out) in block
                .chunks_exact(self.channels)
                .zip(scratch.chunks_exact_mut(INPUT_CHANNELS))
            {
                let pick = |index: usize| frame.get(index).map_or(0.0, |s| f32::from_sample(*s));
                if let Some((l, r)) = self.mic {
                    out[0] = pick(l);
                    out[1] = pick(r);
                }
                if let Some((l, r)) = self.aux {
                    out[2] = pick(l);
                    out[3] = pick(r);
                }
                frames += 1;
            }
            if let Some(ready) = scratch.get(..frames * INPUT_CHANNELS) {
                refused += frames - producer.push_frames(ready);
            }
        }
        refused
    }
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

    fn pair(left: u16, right: u16) -> ChannelPair {
        ChannelPair { left, right }
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
        assert_eq!(choice.routing.master.pair, pair(0, 1));
        assert_eq!(choice.routing.headphones.map(|r| r.pair), Some(pair(2, 3)));
        assert!(!choice.stereo_fallback);
    }

    #[test]
    fn stereo_device_falls_back_without_headphones() {
        let ranges = [range(2, 44_100, 48_000, SampleFormat::F32)];
        let choice = negotiate(&ranges, Some(48_000), &AudioSettings::default()).expect("choice");
        assert_eq!(choice.channels, 2);
        assert_eq!(choice.routing.headphones, None);
        assert!(choice.stereo_fallback);
    }

    #[test]
    fn master_on_high_pair_is_kept_when_it_fits() {
        let settings = AudioSettings {
            master: pair(4, 5),
            headphones: None,
            ..AudioSettings::default()
        };
        let ranges = [
            range(2, 48_000, 48_000, SampleFormat::F32),
            range(8, 48_000, 48_000, SampleFormat::F32),
        ];
        let choice = negotiate(&ranges, None, &settings).expect("choice");
        assert_eq!(choice.channels, 8);
        assert_eq!(choice.routing.master.pair, pair(4, 5));
        assert!(!choice.stereo_fallback);

        let stereo_only = [range(2, 48_000, 48_000, SampleFormat::F32)];
        let choice = negotiate(&stereo_only, None, &settings).expect("choice");
        assert_eq!(choice.routing.master.pair, pair(0, 1));
        assert!(choice.stereo_fallback);
    }

    #[test]
    fn booth_pair_on_a_multichannel_interface() {
        let settings = AudioSettings {
            master: pair(0, 1),
            headphones: Some(pair(2, 3)),
            booth: Some(pair(4, 5)),
            booth_mono: true,
            ..AudioSettings::default()
        };
        let ranges = [range(8, 44_100, 96_000, SampleFormat::F32)];
        let choice = negotiate(&ranges, Some(48_000), &settings).expect("choice");
        assert_eq!(choice.channels, 8);
        assert_eq!(choice.routing.booth.map(|r| r.pair), Some(pair(4, 5)));
        assert!(choice.routing.booth.is_some_and(|r| r.mono));
        assert!(!choice.stereo_fallback);

        // A 4-channel device drops the booth and keeps the headphones.
        let four = [range(4, 44_100, 96_000, SampleFormat::F32)];
        let choice = negotiate(&four, Some(48_000), &settings).expect("choice");
        assert_eq!(choice.routing.booth, None);
        assert_eq!(choice.routing.headphones.map(|r| r.pair), Some(pair(2, 3)));
        assert!(choice.stereo_fallback);
    }

    #[test]
    fn pickers_reach_32_channels_and_nothing_beyond() {
        let settings = AudioSettings {
            master: pair(30, 31),
            headphones: None,
            ..AudioSettings::default()
        };
        let wide = [range(32, 48_000, 48_000, SampleFormat::F32)];
        let choice = negotiate(&wide, None, &settings).expect("choice");
        assert_eq!(choice.channels, 32);
        assert_eq!(choice.routing.master.pair, pair(30, 31));
        assert!(!choice.stereo_fallback);

        // A pair outside the supported range falls back to stereo.
        let settings = AudioSettings {
            master: pair(32, 33),
            headphones: None,
            ..AudioSettings::default()
        };
        let huge = [range(64, 48_000, 48_000, SampleFormat::F32)];
        let choice = negotiate(&huge, None, &settings).expect("choice");
        assert_eq!(choice.routing.master.pair, pair(0, 1));
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
    fn input_needs_the_highest_routed_channel() {
        assert_eq!(input_channels_needed(None, None), None);
        assert_eq!(
            input_channels_needed(Some(InputChannels::mono(2)), None),
            Some(3)
        );
        assert_eq!(
            input_channels_needed(Some(InputChannels::mono(0)), Some(pair(2, 3))),
            Some(4)
        );
        let settings = AudioSettings {
            mic: Some(InputChannels::mono(1)),
            aux: Some(pair(2, 3)),
            ..AudioSettings::default()
        };
        let ranges = [
            range(2, 48_000, 48_000, SampleFormat::F32),
            range(4, 44_100, 48_000, SampleFormat::I16),
        ];
        let choice = negotiate_input(&ranges, 48_000, &settings).expect("choice");
        assert_eq!(choice.channels, 4);
        assert_eq!(choice.format, SampleFormat::I16);
        // The input must run at the output rate: there is no resampler.
        assert_eq!(
            negotiate_input(&ranges, 96_000, &settings),
            Err(InputMismatch::Unsupported { channels_needed: 4 })
        );
        let too_high = AudioSettings {
            mic: Some(InputChannels::mono(40)),
            ..AudioSettings::default()
        };
        assert_eq!(
            negotiate_input(&ranges, 48_000, &too_high),
            Err(InputMismatch::ChannelOutOfRange)
        );
    }

    fn slot(left: usize, right: usize, mono: bool) -> Slot {
        Slot { left, right, mono }
    }

    #[test]
    fn route_places_a_pair_and_leaves_the_rest() {
        let src = [0.5, -0.5, 0.25, -0.25];
        let mut dst = [0.0_f32; 8];
        route_pair(&src, &mut dst, 4, slot(0, 1, false));
        assert_eq!(dst, [0.5, -0.5, 0.0, 0.0, 0.25, -0.25, 0.0, 0.0]);

        let mut dst = [0.0_f32; 8];
        route_pair(&src, &mut dst, 4, slot(2, 3, false));
        assert_eq!(dst, [0.0, 0.0, 0.5, -0.5, 0.0, 0.0, 0.25, -0.25]);

        // Pair outside the device: nothing written, no panic.
        let mut dst = [9.0_f32; 4];
        route_pair(&src, &mut dst, 2, slot(6, 7, false));
        assert_eq!(dst, [9.0; 4]);

        let mut dst = [0_i16; 4];
        route_pair(&src, &mut dst, 2, slot(0, 1, false));
        assert_eq!(dst[0], i16::from_sample(0.5_f32));
    }

    #[test]
    fn mono_plays_the_sum_on_both_channels() {
        let src = [1.0, 0.0, 0.5, -0.5];
        let mut dst = [9.0_f32; 4];
        route_pair(&src, &mut dst, 2, slot(0, 1, true));
        assert_eq!(dst, [0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn input_router_extracts_mic_and_aux_channels() {
        let (mut core, mut handle) = crate::engine(crate::EngineConfig::default()).expect("engine");
        let mut producer = handle.take_input().expect("producer");
        // 3-channel device: mic mono on channel 2, aux pair on channels 0/1.
        let router = InputRouter::new(3, Some(InputChannels::mono(2)), Some(pair(0, 1)));
        let data = [0.1_f32, 0.2, 0.9, 0.3, 0.4, -0.9];
        assert_eq!(router.process(&data, &mut producer), 0);
        handle.set_input_prefill(1);
        let mut out = vec![0.0_f32; 4];
        core.render(&mut out);
        // Mic arrives on both sides, aux is sent to the master as well.
        assert!((out[0] - (0.9 + 0.1)).abs() < 1e-6, "{out:?}");
        assert!((out[1] - (0.9 + 0.2)).abs() < 1e-6, "{out:?}");
        assert!((out[2] - (-0.9 + 0.3)).abs() < 1e-6, "{out:?}");
        assert!((out[3] - (-0.9 + 0.4)).abs() < 1e-6, "{out:?}");

        // Integer input is scaled to full scale 1.0.
        let router = InputRouter::new(1, Some(InputChannels::mono(0)), None);
        assert_eq!(router.process(&[i16::MAX; 2], &mut producer), 0);
    }
}
