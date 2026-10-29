# ADR 0003: Real-time threading and memory model

- Status: accepted
- Date: 2026-10-29
- Deciders: architect

## Context

The audio callback must never allocate, free, lock, block, do I/O, log or panic (ADR 0001). Decoded tracks are held fully in RAM (ADR 0001, deviation 4), so large buffers must reach and leave the audio thread without the audio thread touching the allocator.

## Decision

### Threads

| Thread | Count | Does | Never does |
|---|---|---|---|
| Audio callback (cpal) | 1 | Drains command ring, renders decks, mixer, FX, sampler; writes telemetry and events | Allocate, free, lock, block, I/O, log, panic |
| Control | 1 | Owns `AppCore` state; turns IPC calls and `ControlEvent`s into `EngineCommand`s; reads telemetry; emits UI events; drives LED feedback | Real-time work |
| Loader | 1 | Decodes and resamples a track (media) into an engine-rate buffer; prepares effects and sampler buffers; frees objects returned by the audio thread | Touch engine state directly |
| Analysis pool | cores - 1 (rayon) | Import and analysis jobs (library, analysis) | Run at RT priority |
| MIDI input | 1 (midir callback) | Decodes messages to `ControlEvent`s, pushes them to the control thread | Block |
| Tauri main / webview | 1 | Window and IPC | Wait on the audio thread |

### Rings (all SPSC `rtrb`, preallocated at stream start)

| Ring | From -> to | Item | Capacity | Full |
|---|---|---|---|---|
| Commands | control -> audio | `EngineCommand` (`Copy`, no heap) | 1024 | control thread retries next tick, counts drops |
| Hand-off | loader -> audio | `Box<LoadedTrack>`, `Box<dyn Effect>`, sampler buffers (engine-private enum) | 16 | loader waits |
| Return | audio -> loader | replaced boxes | 64 | audio keeps the old object for the next block and retries; capacity is sized so this never happens in practice |
| Telemetry | audio -> control | `EngineTelemetry` (`Copy`) | 4 | audio overwrites nothing and skips this block's snapshot |
| Events | audio -> control | `EngineEvent` (`Copy`) | 256 | dropped and counted in `EngineTelemetry::dropped_events` |

Moving a `Box` through a ring moves a pointer: no allocation on the audio side. Dropping happens on the loader thread after the box comes back on the return ring (deferred free). `assert_no_alloc` wraps the callback in debug and test builds and aborts on any allocation or free.

### Parameters

Continuous parameters arrive as commands and are smoothed per block inside the engine. `atomic_float` is allowed for single values the UI reads without a command round trip (for example a master meter shown before telemetry starts); no `Mutex` or `RwLock` is ever shared with the audio thread.

### Telemetry rate

The audio thread publishes at most one snapshot per block. The control thread reads the newest one every ~16 ms (~60 Hz) and emits `engine-telemetry` to the UI (ADR 0004). The UI writes it into a non-React store read by `requestAnimationFrame` loops.

### Memory budget

A 5-minute stereo track at 48 kHz in `f32` is about 110 MB. With 4 decks plus one preloading, the budget is about 600 MB of track buffers. The loader refuses tracks longer than a configurable limit (default 30 minutes) with a clear error.

### Testing

Every engine feature has an offline render test (no device) that drives the same render function with commands and compares output. These tests run under `assert_no_alloc`.

## Consequences

- No engine API takes `&mut` engine state from another thread; all mutation is a command.
- Loading a track is asynchronous: the UI sees `PlayState::Loading` until the hand-off is consumed.
- Freeing is delayed until the return ring is drained, so peak memory briefly includes the old buffer.
