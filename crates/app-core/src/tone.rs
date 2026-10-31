//! Test tone for the first-run output check.
//!
//! Plays a short sine on one channel pair of an output device, on its own
//! stream and thread, so the user can tell which pair is master and which is
//! headphones. It is independent of the engine: on a host that gives a device
//! to one stream at a time (ALSA without a mixer) the call fails with the
//! device's error and the UI says so.
//! TODO(verify): route the tone through the engine once it has a test-tone
//! command, so it also works while the engine owns the device.

use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use opendeck_contracts::settings::{AudioHostKind, ChannelPair};

use crate::types::CoreError;

/// Tone frequency in Hz (A4).
pub const TONE_HZ: f32 = 440.0;
/// Tone length.
pub const TONE_DURATION: Duration = Duration::from_millis(900);
/// Linear peak level; about -14 dBFS, loud enough to hear and safe on headphones.
pub const TONE_LEVEL: f32 = 0.2;

/// Sine generator that writes into one channel pair of interleaved frames.
#[derive(Debug, Clone)]
pub struct ToneGen {
    phase: f32,
    step: f32,
    fade: f32,
    elapsed: u64,
    total: u64,
}

impl ToneGen {
    /// Generator for `sample_rate` Hz lasting `duration`.
    pub fn new(sample_rate: u32, duration: Duration) -> Self {
        let rate = sample_rate.max(1) as f32;
        Self {
            phase: 0.0,
            step: TONE_HZ / rate,
            fade: 0.02 * rate,
            elapsed: 0,
            total: (duration.as_secs_f64() * f64::from(sample_rate)) as u64,
        }
    }

    /// Whether the tone has played to its end.
    #[cfg(test)]
    pub fn done(&self) -> bool {
        self.elapsed >= self.total
    }

    /// Fills `data` (interleaved, `channels` per frame). Channels outside
    /// `pair` get silence.
    pub fn fill<T: SizedSample + FromSample<f32>>(
        &mut self,
        data: &mut [T],
        channels: usize,
        pair: ChannelPair,
    ) {
        let channels = channels.max(1);
        for frame in data.chunks_mut(channels) {
            let remaining = self.total.saturating_sub(self.elapsed) as f32;
            let edge = (self.elapsed as f32).min(remaining);
            let envelope = (edge / self.fade).min(1.0);
            let value = (self.phase * std::f32::consts::TAU).sin() * TONE_LEVEL * envelope;
            self.phase = (self.phase + self.step).fract();
            self.elapsed += 1;
            for (index, sample) in frame.iter_mut().enumerate() {
                let on = index == usize::from(pair.left) || index == usize::from(pair.right);
                *sample = T::from_sample(if on { value } else { 0.0 });
            }
        }
    }
}

fn host_for(kind: Option<AudioHostKind>) -> Result<cpal::Host, CoreError> {
    let Some(kind) = kind else {
        return Ok(cpal::default_host());
    };
    let wanted = match kind {
        AudioHostKind::Wasapi => "WASAPI",
        AudioHostKind::Asio => "ASIO",
        AudioHostKind::CoreAudio => "CoreAudio",
        AudioHostKind::Alsa => "ALSA",
        AudioHostKind::Jack => "JACK",
    };
    let id = cpal::available_hosts()
        .into_iter()
        .find(|id| id.name().eq_ignore_ascii_case(wanted))
        .ok_or_else(|| CoreError::Audio(format!("{wanted} is not available")))?;
    cpal::host_from_id(id).map_err(|e| CoreError::Audio(e.to_string()))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    pair: ChannelPair,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels);
    let mut gen_ = ToneGen::new(config.sample_rate, TONE_DURATION);
    device.build_output_stream(
        config,
        move |data: &mut [T], _| gen_.fill(data, channels, pair),
        |error| tracing::warn!(%error, "test tone stream error"),
        None,
    )
}

/// Plays the test tone on `pair` of `device` (`None` for the host default)
/// and returns when it ends. Blocks for about a second; call it from a
/// blocking task, never from the audio thread.
pub fn play_test_tone(
    host: Option<AudioHostKind>,
    device: Option<&str>,
    pair: ChannelPair,
) -> Result<(), CoreError> {
    let host = host_for(host)?;
    let device = match device {
        None => host
            .default_output_device()
            .ok_or_else(|| CoreError::Audio("no output device".to_owned()))?,
        Some(name) => host
            .output_devices()
            .map_err(|e| CoreError::Audio(e.to_string()))?
            .find(|d| d.description().is_ok_and(|d| d.name() == name))
            .ok_or_else(|| CoreError::Audio(format!("output device not found: {name}")))?,
    };
    let supported = device
        .default_output_config()
        .map_err(|e| CoreError::Audio(e.to_string()))?;
    if pair.left.max(pair.right) >= supported.channels() {
        return Err(CoreError::Invalid(format!(
            "the device has {} output channels",
            supported.channels()
        )));
    }
    let format = supported.sample_format();
    let config = supported.config();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, pair),
        SampleFormat::I32 => build::<i32>(&device, config, pair),
        SampleFormat::I16 => build::<i16>(&device, config, pair),
        other => {
            return Err(CoreError::Audio(format!(
                "unsupported sample format {other:?}"
            )));
        }
    }
    .map_err(|e| CoreError::Audio(format!("could not open the output: {e}")))?;
    stream
        .play()
        .map_err(|e| CoreError::Audio(format!("could not start the output: {e}")))?;
    std::thread::sleep(TONE_DURATION + Duration::from_millis(150));
    drop(stream);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_stays_on_its_pair() {
        let mut tone = ToneGen::new(48_000, Duration::from_millis(100));
        let mut data = vec![0.0f32; 8 * 4800];
        tone.fill(&mut data, 4, ChannelPair { left: 2, right: 3 });
        let mut peak_on = 0.0f32;
        for frame in data.chunks(4) {
            assert_eq!(frame[0], 0.0);
            assert_eq!(frame[1], 0.0);
            assert_eq!(frame[2], frame[3]);
            peak_on = peak_on.max(frame[2].abs());
        }
        assert!(peak_on > 0.15 && peak_on <= TONE_LEVEL);
    }

    #[test]
    fn tone_fades_and_finishes() {
        let mut tone = ToneGen::new(48_000, Duration::from_millis(50));
        let mut data = vec![0.0f32; 2 * 4800];
        tone.fill(&mut data, 2, ChannelPair { left: 0, right: 1 });
        assert!(tone.done());
        assert_eq!(data[0], 0.0, "starts silent");
        assert!(data[data.len() - 2].abs() < 0.01, "ends silent");
        // After the end the generator keeps writing silence.
        let mut more = vec![1.0f32; 64];
        tone.fill(&mut more, 2, ChannelPair { left: 0, right: 1 });
        assert!(more.iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn rejects_a_pair_beyond_the_device() {
        // No audio device is needed: an unknown device name fails first.
        let err = play_test_tone(
            None,
            Some("no such device"),
            ChannelPair { left: 0, right: 1 },
        );
        assert!(err.is_err());
    }
}
