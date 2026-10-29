# analysis

Owner: analysis-dev.

## Purpose
Offline, deterministic analysis: BPM and beat grid, musical key (Camelot), waveform peaks at several zoom levels with 3-band colour data, EBU R128 loudness and true peak.

## Owned paths
- `crates/analysis/**`
- `crates/analysis/tests/fixtures/**` (generated or licence-clean audio only)

## Depends on
- `opendeck-contracts`: `Analyzer`, `AudioView`, `AnalysisResult`, `AnalysisFeatures`, `CancelFlag`, `BeatGrid`, `MusicalKey`, `Loudness`, `PeakLevel`/`PeakPoint`.
- `opendeck-media` for decoding in tests and tools.

## External crates (add when used)
- `stratum-dsp` 1 (evaluate first, M3 spike), `rustfft` 6.4, `ebur128` 0.1, `rayon` (callers own the pool). Rejected: `aubio-rs` (GPL).

## Rules
- Each stage is an `Analyzer` with a stable `id` and `version`; bump `version` (and `ANALYSIS_VERSION`) whenever output changes for the same input.
- Check `CancelFlag` at least every ~100 ms of work; report monotonic progress.
- No threads spawned inside an analyzer; parallelism is the pipeline's job.
- Peaks: `contracts::peaks` v1 layout. Document band crossovers here when chosen. Proposed levels: 0 = 64 frames/point, 1 = 256, 2 = 1024, overview = fixed 2048 points (confirm with ui-viz).

## Acceptance per milestone
- M2: peaks analyzer (all levels) and loudness analyzer; a 5-minute track analysed in under 1 s release build on one core.
- M3: spike `stratum-dsp` on a labelled set: BPM within 0.1 and key exact or relative on at least 90 % (report numbers). Pass: wrap it. Fail: custom onset + autocorrelation tempo and chroma + Krumhansl-Schmuckler key behind the same trait. Beat grid anchored on a downbeat.
- M3: deterministic across Linux and Windows (CI compares hashes of results).

## Open questions
- Labelled test set: source and licence (no copyrighted audio in the repo).
- BPM range folding (e.g. 70-180) as a user setting or fixed.
- Band crossover frequencies for the colour waveform.
