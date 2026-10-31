# ADR 0008: CLAP and VST3 plugin hosting

- Status: proposed (design and skeleton, milestone 9a)
- Date: 2026-10-31
- Deciders: architect, owner

## Context

Milestone 9a adds third-party CLAP and VST3 plugins as effects in the existing FX slots and as instruments played from pads and MIDI notes. ADR 0006 already put the plugin boundary at `EffectFactory` and left crash isolation open. ADR 0001 allows only permissive or unmodified MPL-2.0 dependencies; GPL, LGPL and AGPL are rejected.

## Licence findings (checked 2026-10-31)

| Item | Licence | Source |
|---|---|---|
| VST 3 SDK (`steinbergmedia/vst3sdk`, SDK 3.8.x) | MIT, "Copyright (c) 2026, Steinberg Media Technologies GmbH" | `LICENSE.txt` and README section "License & Usage guidelines" of the repo: "VST 3 SDK is under MIT license"; "Licensing under GPLv3 and the Steinberg proprietary license is no longer available." |
| VST 3 interface headers (`steinbergmedia/vst3_pluginterfaces`) | MIT | `LICENSE.txt` of the repo |
| VST trademark and logo | Not covered by MIT | Same README, "Trademark and Logo Usage": optional; if used, unaltered logo, product contexts only; "VST" must not appear in product or company names. |
| `vst3` crate 0.3.0 (coupler-rs/vst3-rs) | MIT OR Apache-2.0 | crates.io API and `Cargo.toml`/`LICENSE-*` in the crate source. Bindings are pre-generated from the MIT headers (fork `coupler-rs/vst3_pluginterfaces`, `LICENSE.txt` MIT, Steinberg 2025); no SDK checkout at build time (crate CHANGELOG 0.3.0). Dependency `com-scrape-types` 0.1.1: MIT OR Apache-2.0. |
| `vst3-sys` (RustAudio) | GPL-3.0 | `Cargo.toml` of `RustAudio/vst3-sys` (`license = "GPL-3.0"`); not published on crates.io. **Rejected.** |
| CLAP (`free-audio/clap`) | MIT | `LICENSE` of the repo |
| `clap-sys` 0.5.0 | MIT/Apache-2.0 | crates.io API, `LICENSE-*` in the crate source |
| `clack-host` / `clack-common` 0.2.0 (prokopyl/clack) | MIT OR Apache-2.0 | crates.io API, `LICENSE-*` in the crate source. Pulls `libloading` 0.8 (ISC) and, on macOS, `objc2-foundation` (MIT). |
| `vst3-host` 0.10 | MIT | crates.io API. Not chosen: brings its own audio playback, egui and window code; one maintainer. |
| `plughost` 0.2 | MIT OR Apache-2.0 | crates.io API. Not chosen: 196 downloads, every plugin out of process, JSON IPC. |
| `rack` 0.4 | MIT OR Apache-2.0 | crates.io API. Not chosen: no CLAP, last release 2025-11. |

`cargo deny --all-features check licenses` passes with the chosen crates. Because the VST3 bindings are derived from Steinberg's MIT headers, the Steinberg MIT notice goes into the third-party notices of any build with the `vst3` feature. OpenDeck does not use the VST logo for now (owner decision if wanted).

## Decision

### Crates

- New crate `crates/plugin-host` (`opendeck-plugin-host`), depending on `contracts` only (plus `rtrb`, `serde`, `serde_json`, `thiserror`). app-core depends on it; the engine does not.
- Format bindings, each behind a default-off cargo feature: `clap` -> `clack-host` 0.2, `vst3` -> `vst3` 0.3. Both build on Linux without native SDKs; the features are default-off so the default build stays small until wave 9a-1 ships.
- We write our own host layer on the low-level crates. The engine's slot host already owns wet/dry, bypass, tails and latency compensation (ADR 0006), so a full hosting library would duplicate it.
- Format code is FFI. The workspace denies `unsafe_code`; the format modules (`clap.rs`, `vst3.rs`, later their submodules) carry a module-level `allow(unsafe_code)` with a `SAFETY` comment per block. The rest of the crate stays safe.

### Process model

- **Scanning out of process.** `opendeck-plugin-scan <provider> <path>` (a binary of the crate, shipped next to the app) loads one file, lists its plugins, validates each (instantiate, activate 48 kHz/512, 8 blocks with an impulse or a note, finite output, deactivate) and prints a JSON `ScanReport`. Crash or signal -> `Crashed`; no exit in 30 s -> killed, `TimedOut`. Both blacklist the file until the user retries or the file changes (size or mtime). The app never loads a plugin that has not passed a scan.
- **Processing in process.** The plugin's audio half runs inside the engine slot on the audio thread. Reason: a DJ app runs at 128 to 512 frame buffers; an out-of-process bridge costs one context switch pair per block per plugin and either a block of latency or a tight cross-process wait inside the audio callback, plus shared memory, process supervision and editor window reparenting across processes on three OSes. Cost: a plugin that crashes while processing takes the app down.
- **Runtime crash mitigation.** app-core writes a session marker listing the active plugin keys before audio starts and clears it on clean exit. After an unclean exit the next launch does not auto-restore plugin slots and shows which plugins were active, so the user can blacklist one. Plugins are not blacklisted automatically on runtime crashes (the cause may be elsewhere).
- **Later option.** An out-of-process "sandboxed" instance can implement the same `Effect` trait (shared memory + one wait per block) for plugins the user marks unstable. Not in 9a.

### Threads

| Work | Thread |
|---|---|
| Catalog, factories, `create`, `prepare`, drop, `InstanceHandle` | control |
| `PluginBackend::instantiate`, every `PluginInstance` call (CLAP `[main-thread]`, VST3 controller and UI calls), editors | plugin main thread = platform UI thread |
| `process`, `set_parameter`, `reset` | audio |
| `scan_file` | scanner subprocess |

- src-tauri implements `MainThread` with Tauri's `run_on_main_thread`. Instances are created there and stay there in a thread-local table; other threads reach them by queuing jobs. The control thread waits at most 10 s per job; the main thread must never wait on the control thread.
- `Effect::prepare` (control thread) deactivates and activates the instance on the main thread and takes the new audio half. If that fails, an effect passes its input through and an instrument is silent; the handle reports `Failed`.
- The engine drops returned boxes off the audio thread (ADR 0003); the adapter's drop queues deactivate and destroy on the main thread.
- Parameters: `set_parameter` queues into a preallocated list (256 per block, coalesced by id). Editor edits arrive on an SPSC ring (`rtrb`, 1024) main -> audio; changes the plugin makes go audio -> main on a second ring for UI feedback and preset dirty state. Values are normalized `[0, 1]` (VST3 native; CLAP converts with the descriptor).
- Parameter ids: CLAP `clap_id` and VST3 `ParamID` are `u32`, used directly as `ParamId`. Parameters with an empty range are hidden. `ParamUnit::None`, `ParamCurve::Linear` (or `Stepped` for CLAP stepped params); value text comes from the plugin in wave 9a-5.

### Latency, sample rate, block size

- Latency is read at activation and returned by `latency_frames`; the engine compensates the dry path per slot at install (ADR 0006 addendum). When a plugin reports a latency change (CLAP `latency.changed` + restart request, VST3 `restartComponent(kLatencyChanged)`), app-core saves state, recreates the instance and re-installs the slot.
- A sample-rate or buffer change re-prepares off the audio thread as for built-ins (ADR 0006). For plugins app-core saves the state, creates a new instance, restores the state, prepares and swaps.
- Plugins always get stereo in/out, 32-bit float. Plugins without a stereo main port are not listed (mono plugins: post-MVP, duplicate channels).

### Editors (wave 9a-3)

- `PluginInstance` gains `open_editor(parent)` / `close_editor`. Parent handles: Win32 `HWND`, Cocoa `NSView*`, X11 window id, from a Tauri child window via `raw-window-handle` (MIT/Apache, already in the Tauri tree). CLAP `gui` extension (embedded, falls back to floating), VST3 `IPlugView` with `IPlugFrame::resizeView`. Wayland is not supported by either format's embedding; on Linux the editor window uses X11/XWayland.

### Instruments

- Contract: `contracts::instrument::{Instrument, InstrumentFactory, InstrumentRegistry, InstrumentInfo, NoteEvent, NoteKind, MAX_NOTE_EVENTS}` (added in this ADR, additive). `Instrument` mirrors `Effect` with no input and a `&[NoteEvent]` per block.
- **MVP shape: instrument slots.** The engine gets `INSTRUMENT_SLOTS` (proposed 2) slots holding a `Box<dyn Instrument>`, installed like effects (`EngineHandle::install_instrument`, hand-off and return rings). Their output joins the sampler bus (sampler level, PFL, before the master FX unit). A sampler pad can be assigned to "instrument slot N, key K": a pad press sends note-on, release note-off, velocity 1.0 (FLX4 pads have no velocity, `TODO(verify)` other controllers). External MIDI keyboards route notes to a slot (wave 9a-4, midi crate).
- Engine contract additions for 9a-4 (proposed, not made now; need owner/architect approval): `EngineCommand::InstrumentNote { slot, key, velocity, on }` (fits the 40-byte limit), `SetInstrumentGain`, `InstrumentSlotId` (bounded id), `EngineTelemetry::instruments` (per slot `{loaded, active_notes, level}`).
- Also offered: "Render to sampler slot", which renders a held note offline through the same `Instrument` trait into a `TrackBuffer` (useful for one-shots with sampler sync and quantize).

### Identity, catalog, presets

- Plugin key: `EffectKey { provider: "clap" | "vst3", id }`, id = CLAP descriptor id or the VST3 class id as 32 uppercase hex digits. Paths are not part of the key. Duplicate ids in two files: the newer version wins, then the earlier folder in the folder list.
- `contracts::plugin::{PluginInfo, PluginKind, PluginScanStatus}` describe scan results for app-core and the UI. `plugin_host::scan::ScanRecord` (path, size, mtime, status, plugins) is persisted by app-core as JSON in library settings (`plugins.scan`, `plugins.folders`).
- Plugin state: `InstanceHandle::save_state` returns opaque bytes (CLAP `state`; VST3 component state followed by controller state, length-prefixed). Library wave: add `state BLOB NULL` to `fx_presets` and the saved-set slot table (migration, ADR 0005 process). `FxPreset` itself is unchanged in 9a; app-core keeps plugin state next to it until the migration lands.

### Folders and platforms

- Default folders follow the formats: CLAP from the comment in `clap/entry.h` plus `CLAP_PATH`; VST3 from the SDK hosting code (`module_win32.cpp`, `module_mac.mm`, `module_linux.cpp`). Both are in `plugin_host::scan`. Users add folders in scan settings.
- 64-bit only: x86_64 on Windows and Linux, arm64 and x86_64 on macOS (a universal app loads the slice of its own architecture; an x86_64-only plugin does not load in the arm64 app and is reported as a failed scan).
- Windows is primary, macOS second, Linux best effort. AU, LV2, VST2 and 32-bit bridging are out of scope.

## Consequences

- Plugins and built-ins stay identical to the engine. `Effect` and `EffectInfo` did not change.
- A plugin crash at scan time is contained; at run time it is not (mitigated by the session marker). Revisit with the sandboxed instance if users report crashes.
- All plugin main-thread work competes with the UI thread. Long plugin loads freeze the window briefly; instantiation is done one plugin at a time with a spinner in the UI.
- Plugins can allocate or lock inside `process`; the RT rules cannot be enforced on foreign code. CPU and xrun telemetry show the cost.
- The format modules contain `unsafe` code, the first in the workspace outside dependencies.

## Owner decisions needed

1. In-process processing with out-of-process scanning (recommended) versus everything out of process.
2. Instrument MVP: 2 live instrument slots on the sampler bus plus "render to sampler slot" (recommended).
3. Whether to show the VST logo (trademark rules apply) or only the text "VST3 plugins".
4. Linux plugin support: best effort, X11 editors only.
