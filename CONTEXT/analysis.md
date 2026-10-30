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
- `rustfft` 6.4 (scalar planner, for cross-platform identical arithmetic), `ebur128` 0.1. `rayon` is left to callers. Rejected: `aubio-rs` (GPL), `stratum-dsp` 1.0 (failed the M3 spike, see Accuracy).

## Rules
- Each stage is an `Analyzer` with a stable `id` and `version`; bump `version` (and `ANALYSIS_VERSION`) whenever output changes for the same input.
- Check `CancelFlag` at least every ~100 ms of work; report monotonic progress.
- No threads spawned inside an analyzer; parallelism is the pipeline's job.
- Peaks: `contracts::peaks` v1 layout. Levels: 0 = 64 frames/point, 1 = 256, 2 = 1024, overview = `ceil(frames / 2048)` frames/point (about 2048 points; confirm with ui-viz). The signal is `(L + R) / 2`; each point holds the max abs of the full band and of three bands. Crossovers: 200 Hz and 2.5 kHz, 2nd-order Butterworth (low = LP200, mid = HP200 then LP2500, high = HP2500).

## Acceptance per milestone
- M2: peaks analyzer (all levels) and loudness analyzer; a 5-minute track analysed in under 1 s release build on one core.
- M3 (done): tempo/beat grid and key analyzers, custom, behind the `Analyzer` trait. `stratum-dsp` was spiked and rejected. The anchor is the first downbeat at or after 0 s (4/4, constant tempo). The BPM range defaults to 70-180 and is configurable (`TempoConfig`).
- M3 (done): all four analyzers on a 5-minute stereo 44.1 kHz track take about 1.8 s on one core, release (peaks 0.20 s, loudness 0.60 s, tempo 0.65 s, key 0.30 s). Peaks are one streaming pass with no copies.
- M3 (open): cross-platform determinism is by construction (scalar FFT planner, f64 filters with f32-rounded coefficients, rounded outputs: BPM 0.01, anchor 0.1 ms, LUFS and dBTP 0.01 dB, peaks u8). The CI hash comparison on Windows is not wired yet.

## Accuracy
Synthetic labelled set in `tests/common/mod.rs`, checked by `tests/accuracy.rs` (run with `--release -- --nocapture` to see the tables).

- Tempo set: 28 generated 40 s drum patterns (kick every beat with a bar accent, snare on 2 and 4, swung hats, noise bed, timing jitter) at 90, 100, 120, 124, 128, 140, 150, 174 BPM in 3 conditions, plus 98.6, 123.45, 132.8 and 87.3.
- Key set: 24 keys (12 tonics, major and minor, detuned up to 25 cents), 24 s of I-IV-V-I or i-iv-V-i pads with harmonics.

| Method | BPM within 0.1 | Anchor within 10 ms | Key exact or relative | Key exact |
|---|---|---|---|---|
| `stratum-dsp` 1.0, default config | 8/28 (29 %) | 0/28 | 8/24 (33 %) | 8/24 |
| Custom (shipped) | 28/28 (100 %) | 28/28, worst error 1.0 ms | 24/24 (100 %) | 24/24 |

`stratum-dsp` also uses rayon internally and took 0.7 to 1.8 s per 40 s clip, so it broke the threading and speed rules too. The synthetic set is easy (clean kicks, clear tonality): treat the custom numbers as a regression guard, not a real-music score. Known limits: constant tempo only, 4/4 assumed, downbeat picked by low-band strength, octave choice is a heuristic (prior centred on 130 BPM plus a kick-at-half-beat doubling check).

## Open questions
- A licence-clean real-music labelled set to complement the synthetic one.
- Variable-tempo tracks (segments, needs a contracts change).
- Windows hash check in CI.
