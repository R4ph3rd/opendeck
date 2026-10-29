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
- `cpal` 0.18 (WASAPI default; `asio` cargo feature), `rtrb` 0.4, `signalsmith-stretch` 0.1 (key lock, key shift), `thiserror` 2, `assert_no_alloc` 1 (optional, dev). Later: `rubato` (async resampler for dual-device routing), `atomic_float` 1.

## RT rules (ADR 0003)
- Callback never allocates, frees, locks, blocks, does I/O, logs (`tracing`) or panics. No `unwrap`, no indexing that can fail: use validated ids and `get`.
- All mutation through the command ring; heap objects arrive on the hand-off ring and leave on the return ring.
- Smooth every continuous parameter per block. Clamp out-of-range values.
- Offline render tests wrap the render call in `assert_no_alloc`.
- Effects: host owns wet/dry, bypass crossfade, tail handling and latency compensation (ADR 0006).

## Acceptance per milestone
- M1: one deck plays a file, device selection, offline render tests. Track in as `TrackBuffer` over the hand-off ring (no decoding in the engine); play/pause, CUE (CDJ behaviour), seek, tempo fader and ranges, pitch bend, key lock; cpal output with device, rate and buffer selection, 4-channel routing (master 1/2, headphones 3/4 silent) with stereo fallback; telemetry at ~60 Hz; `OfflineRenderer`; zero allocations in the callback. The engine already allocates N decks (2 by default).
- M2: two decks, mixer (trim, 3-band EQ, filter, fader, crossfader curves), routing with cue (PFL, headphone mix, split cue), beat and tempo sync with leader election, quantize, loops and loop roll, hot cues, beat jump, slip, jog scratch and nudge.
- M5: FX units (deck/master/send), chains of `FX_SLOTS_PER_UNIT`, beat-synced context.
- M7: sampler slots, key shift via stretcher.

## M1 implementation notes
- `engine(config)` returns `(EngineCore, EngineHandle)`. The core goes to the cpal callback (`device::open_output`) or to `OfflineRenderer`; the handle stays on the control thread.
- Tracks: `EngineHandle::load_track` pushes a `TrackBuffer` (`Arc<[f32]>`, interleaved stereo at engine rate) on the hand-off ring. Replaced or ejected buffers return on the return ring; call `reclaim()` to drop them off the audio thread.
- Jumps (seek, cue return, pause, key lock toggle) render a 2 ms fade-out tail of the old state over a 2 ms fade-in of the new one, so they do not click.
- Key lock: Signalsmith stretcher, primed on every start or jump so the audible playhead matches `pos` despite its ~120 ms latency at 48 kHz. Off: varispeed with linear or cubic (`EngineConfig::interpolation`).
- Backend errors (device lost, reroute) are reported as `device::AudioEvent`. `EngineEvent` has no variant for them yet: contract change proposed.
- cargo features: `asio` (forwards `cpal/asio`, Windows), `alloc-guard` (wraps the callback in `assert_no_alloc`; the binary installs `AllocDisabler` in debug).

## Open questions
- Maximum pitch bend amount and jog nudge sensitivity: product tuning. M1 uses `MAX_PITCH_BEND` = 8 % (`TODO(verify)`); jog nudge arrives in M2.
- Filter topology (slope, resonance) for the channel filter: needs a spec before M2 mixer work.
- EQ type (isolator vs shelving) and kill behaviour: needs a spec.
