# engine

Owner: engine-dev.

## Purpose
Real-time audio engine: decks (2 shown, 4 supported), transport, CUE behaviour, tempo and key lock, sync, jog scratch and nudge, loops and loop roll, hot cues, beat jump, slip, mixer (trim, 3-band EQ, filter, fader, PFL, crossfader curves), master and headphone outputs (split cue, cue/master mix), FX slot host, sampler, offline renderer.

## Owned paths
- `crates/engine/**`

## Depends on
- `opendeck-contracts`: `EngineCommand`, `EngineTelemetry`, `EngineEvent`, `Effect`, `ProcessContext`, `StereoBufferMut`, `BeatGrid`, `CrossfaderCurve`, `TempoRange`, ids, units.
- `opendeck-media`: decoding on the loader thread only.

## External crates (add when used)
- `cpal` 0.18 (WASAPI default; `asio` and `jack` cargo features), `rtrb` 0.4, `signalsmith-stretch` 0.1 (key lock, key shift), `rubato` (async resampler only for future dual-device routing), `atomic_float` 1. Dev: `assert_no_alloc` 1.

## RT rules (ADR 0003)
- Callback never allocates, frees, locks, blocks, does I/O, logs (`tracing`) or panics. No `unwrap`, no indexing that can fail: use validated ids and `get`.
- All mutation through the command ring; heap objects arrive on the hand-off ring and leave on the return ring.
- Smooth every continuous parameter per block. Clamp out-of-range values.
- Offline render tests wrap the render call in `assert_no_alloc`.
- Effects: host owns wet/dry, bypass crossfade, tail handling and latency compensation (ADR 0006).

## Acceptance per milestone
- M1: cpal output on default device, 4-channel routing (master 1/2, cue 3/4) with stereo fallback; load, play, pause, CUE (CDJ behaviour), seek, tempo fader with ranges, key lock, pitch bend, jog scratch/nudge, full channel strip and crossfader curves, headphone mix and split cue; telemetry at block rate; offline tests for each; zero allocations in callback.
- M3: beat and tempo sync with leader election, quantize, phase-aligned play start.
- M4: auto/manual loops, loop roll with slip-style resume, hot cues (set/trigger/preview), beat jump keeping phase, slip mode.
- M5: FX units (deck/master/send), chains of `FX_SLOTS_PER_UNIT`, beat-synced context.
- M7: sampler slots, key shift via stretcher.

## Open questions
- Maximum pitch bend amount and jog nudge sensitivity: product tuning, define in M1 with the owner.
- Filter topology (slope, resonance) for the channel filter: needs a spec before M1 mixer work.
- EQ type (isolator vs shelving) and kill behaviour: needs a spec.
