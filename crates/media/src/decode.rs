//! File decoding to interleaved stereo `f32` and offline resampling.

use std::fs::File;
use std::path::Path;

use audioadapter_buffers::direct::InterleavedSlice;
use opendeck_contracts::SampleRate;
use opendeck_contracts::analysis::CancelFlag;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::MediaError;

/// Decoded audio: interleaved stereo `f32` at `sample_rate`.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedAudio {
    /// Interleaved L, R samples. `samples.len() == frames * 2`.
    pub samples: Vec<f32>,
    /// Rate of `samples` (the requested target rate).
    pub sample_rate: SampleRate,
    /// Frames per channel.
    pub frames: usize,
    /// Rate of the file before resampling.
    pub source_rate: SampleRate,
    /// Channel count of the file before up/downmix.
    pub source_channels: u16,
}

/// Borrowed interleaved stereo view for analysis. No copies.
#[derive(Debug, Clone, Copy)]
pub struct AudioView<'a> {
    /// Rate of `samples`.
    pub sample_rate: SampleRate,
    /// Interleaved L, R samples; even length.
    pub samples: &'a [f32],
}

impl AudioView<'_> {
    /// Frames per channel.
    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    /// Iterator over one channel (0 = left, 1 = right); any other index is
    /// treated as right.
    pub fn channel(&self, ch: usize) -> impl Iterator<Item = f32> + '_ {
        self.samples.iter().skip(ch.min(1)).step_by(2).copied()
    }

    /// Mono mix `(L + R) / 2`, one value per frame.
    pub fn mono(&self) -> impl Iterator<Item = f32> + '_ {
        self.samples.chunks_exact(2).map(|f| 0.5 * (f[0] + f[1]))
    }

    /// Copies to planar channels (for planar consumers such as the contracts
    /// `AudioView`).
    pub fn to_planar(&self) -> [Vec<f32>; 2] {
        [self.channel(0).collect(), self.channel(1).collect()]
    }
}

impl DecodedAudio {
    /// Borrowed view for analyzers.
    pub fn view(&self) -> AudioView<'_> {
        AudioView {
            sample_rate: self.sample_rate,
            samples: &self.samples,
        }
    }
}

fn sym_err(e: SymError) -> MediaError {
    match e {
        SymError::Unsupported(m) => MediaError::Unsupported(m.to_string()),
        other => MediaError::Corrupt(other.to_string()),
    }
}

/// Appends `src` (interleaved, `ch` channels) to `out` as stereo.
fn push_stereo(out: &mut Vec<f32>, src: &[f32], ch: usize) {
    match ch {
        0 => {}
        1 => out.extend(src.iter().flat_map(|&s| [s, s])),
        2 => out.extend_from_slice(src),
        // TODO(verify): for more than two channels only the first two (front
        // left/right in the common layouts) are kept; no centre/surround fold.
        _ => out.extend(src.chunks_exact(ch).flat_map(|f| [f[0], f[1]])),
    }
}

/// Decodes `path` to interleaved stereo `f32` at `target_rate`.
///
/// Mono is duplicated; more than two channels keep the first two. When the
/// file rate differs from `target_rate` the audio is resampled offline with
/// rubato's FFT resampler. Output length is `round(source_frames * target /
/// source)` frames. Corrupt packets are skipped; a truncated file returns what
/// decoded before the break. Checks `cancel` between packets.
pub fn decode_file(
    path: &Path,
    target_rate: SampleRate,
    cancel: &CancelFlag,
) -> Result<DecodedAudio, MediaError> {
    if target_rate.0 == 0 {
        return Err(MediaError::Resample("target rate is 0".into()));
    }
    let file = File::open(path).map_err(|source| MediaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(sym_err)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(MediaError::Empty)?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or(MediaError::Empty)?;
    let track_id = track.id;
    let est_frames = track.num_frames.unwrap_or(0).min(1 << 26) as usize;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(sym_err)?;

    let mut out: Vec<f32> = Vec::with_capacity(est_frames * 2);
    let mut scratch: Vec<f32> = Vec::new();
    let mut source_rate = params.sample_rate.unwrap_or(0);
    let mut source_channels = 0usize;
    let mut bad_packets = 0u32;
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            // End of stream, truncated file or unrecoverable read error:
            // keep what we have.
            Ok(None) | Err(_) => break,
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let spec = buf.spec();
                source_rate = spec.rate();
                let ch = spec.channels().count();
                source_channels = ch;
                buf.copy_to_vec_interleaved(&mut scratch);
                push_stereo(&mut out, &scratch, ch);
            }
            Err(SymError::DecodeError(_)) => {
                bad_packets += 1;
                if bad_packets > 1000 && out.is_empty() {
                    return Err(MediaError::Corrupt("too many undecodable packets".into()));
                }
            }
            Err(_) => break,
        }
    }
    if out.is_empty() || source_rate == 0 {
        return Err(MediaError::Empty);
    }
    let source = SampleRate(source_rate);
    let samples = if source == target_rate {
        out
    } else {
        resample(out, source, target_rate, cancel)?
    };
    Ok(DecodedAudio {
        frames: samples.len() / 2,
        samples,
        sample_rate: target_rate,
        source_rate: source,
        source_channels: u16::try_from(source_channels).unwrap_or(u16::MAX),
    })
}

/// Resamples interleaved stereo with the FFT resampler (offline, fixed ratio).
fn resample(
    input: Vec<f32>,
    from: SampleRate,
    to: SampleRate,
    cancel: &CancelFlag,
) -> Result<Vec<f32>, MediaError> {
    if cancel.is_cancelled() {
        return Err(MediaError::Cancelled);
    }
    let in_frames = input.len() / 2;
    let err = |e: &dyn std::fmt::Display| MediaError::Resample(e.to_string());
    let mut resampler = Fft::<f32>::new(from.0 as usize, to.0 as usize, 1024, 2, FixedSync::Both)
        .map_err(|e| err(&e))?;
    let adapter = InterleavedSlice::new(&input, 2, in_frames).map_err(|e| err(&e))?;
    let out = resampler
        .process_all(&adapter, in_frames, None)
        .map_err(|e| err(&e))?;
    let mut data = out.take_data();
    let want = ((in_frames as f64) * to.as_f64() / from.as_f64()).round() as usize;
    data.truncate(want * 2);
    Ok(data)
}
