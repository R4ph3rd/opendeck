# media

Owner: analysis-dev.

## Purpose
Decode audio files to PCM, resample to a target rate, read tags and artwork. Shared by the engine loader thread and the analysis/import pipeline. Never runs on the audio thread.

## Owned paths
- `crates/media/**`

## Depends on
- `opendeck-contracts` (units: `SampleRate`, `Frames`, `Seconds`).

## External crates (add when used)
- `symphonia` 0.6 (MPL-2.0, unmodified): MP3, AAC/M4A, ALAC, FLAC, WAV, AIFF, Ogg Vorbis. Enable only the needed codec features.
- `rubato` 5 (MIT): file rate to engine rate at load time (sinc, high quality, offline).
- `lofty` 0.25: tags and embedded artwork.

## Interfaces
- Done (M1): `decode_file(path, target_rate: SampleRate, cancel: &CancelFlag) -> Result<DecodedAudio, MediaError>`: interleaved stereo `f32` (mono duplicated, more than two channels keep the first two), `sample_rate`, `frames`, `source_rate`, `source_channels`. `DecodedAudio::view()` gives a borrowed `AudioView` (interleaved, no copy; `to_planar()` for planar consumers).
- Done (M1, tags early): `read_tags(path) -> Result<TrackTags, MediaError>` (title, artist, album, genre, year, comment, duration, bitrate, sample rate, channels, optional artwork mime and bytes).
- M3: bpm/key tags, separate `read_artwork`, codec name, for the import pipeline and analysis view.
- Later: streaming variant with a progress callback (loader shows `PlayState::Loading`).
- `SUPPORTED_EXTENSIONS` / `is_supported_extension` (exists).

## Rules
- Deterministic output: same file, same samples on every OS.
- Errors are typed (`thiserror`), never panics on corrupt files. Fuzz-ish tests with truncated files.
- Memory: decode directly into the final buffer when possible; at most one extra copy for resampling.

## Acceptance per milestone
- M1 (done): decode + resample to 48 kHz; length within 1 frame of `round(src * target / src_rate)`; 5-minute 44.1 kHz stereo WAV decodes and resamples in about 0.46 s (release, ignored test `bench_five_minutes_44k_to_48k`). Fixtures are generated in tests: WAV, AIFF, FLAC (verbatim writer). MP3, AAC/ALAC and Ogg are enabled but have no encoder-free fixtures yet.
- M3: tags and artwork used by the import pipeline and analysis view; `AudioView` used by analysis without copies; bpm/key tags; lossy-format fixtures.
- Later: cancellation within 100 ms for very long files (currently checked per packet, before resampling).

## Open questions
- Resampler delay: rubato's FFT resampler leaves a constant lead of about half an output frame (10 us at 48 kHz). Negligible for cues; compensate if sub-frame alignment is ever needed.
- Gapless metadata (encoder delay/padding) for MP3/AAC: needed for sample-accurate cue positions across apps. Check symphonia support.
- Fixture generation strategy for lossy formats without shipping third-party audio.
