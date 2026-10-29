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

## Interfaces to provide
- `decode_file(path, target_rate: Option<SampleRate>, cancel: &CancelFlag) -> Result<DecodedAudio>`: planar stereo `Vec<f32>` (mono duplicated), rate, frame count. Exposes `AudioView` for analyzers.
- Streaming variant with a progress callback (loader shows `PlayState::Loading`).
- `read_tags(path) -> Result<TagInfo>` (title, artist, album, genre, year, comment, bpm, key, duration, bitrate, sample rate, channels, codec) and `read_artwork(path) -> Option<(mime, bytes)>`.
- `SUPPORTED_EXTENSIONS` / `is_supported_extension` (exists).

## Rules
- Deterministic output: same file, same samples on every OS.
- Errors are typed (`thiserror`), never panics on corrupt files. Fuzz-ish tests with truncated files.
- Memory: decode directly into the final buffer when possible; at most one extra copy for resampling.

## Acceptance per milestone
- M1: decode + resample WAV/FLAC/MP3 fixtures (generated in tests, no copyrighted audio) to 48 kHz; sample-accurate length; decode a 5-minute file in under 2 s release build.
- M2: tags and artwork for all formats; AAC/ALAC/AIFF/Ogg fixtures; cancellation within 100 ms.
- M3: `AudioView` used by analysis without copies.

## Open questions
- Gapless metadata (encoder delay/padding) for MP3/AAC: needed for sample-accurate cue positions across apps. Check symphonia support.
- Fixture generation strategy for lossy formats without shipping third-party audio.
