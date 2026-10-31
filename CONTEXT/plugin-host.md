# plugin-host

Owner: plugin-host-dev.

## Purpose
Host CLAP and VST3 plugins as effects in the engine FX slots and as instruments in instrument slots, behind the contracts `EffectFactory` and `InstrumentFactory`. Discover plugins in a scanner subprocess; blacklist files that crash or hang it. Design: `docs/adr/0008-plugin-hosting.md`.

## Owned paths
- `crates/plugin-host/**`

## Depends on
- `opendeck-contracts` only (`Effect`, `EffectFactory`, `EffectInfo`, `ParamDescriptor`, `ProcessContext`, `StereoBufferMut`, `Instrument`, `InstrumentFactory`, `NoteEvent`, `PluginInfo`, `PluginKind`, `PluginScanStatus`). Never the engine or app-core.

## External crates
- `rtrb` 0.4, `serde`/`serde_json`, `thiserror` 2.
- Feature `clap`: `clack-host` 0.2 (MIT OR Apache-2.0; pulls `clap-sys` MIT/Apache-2.0, `libloading` ISC).
- Feature `vst3`: `vst3` 0.3 (MIT OR Apache-2.0, bindings from the MIT VST 3 SDK headers; ship Steinberg's MIT notice).
- Rejected: `vst3-sys` (GPL-3.0). Any new dependency: record the licence, run `cargo deny --all-features check licenses`.

## Rules
- Format modules (`clap*`, `vst3*`) may use `unsafe` with a module-level `allow(unsafe_code)` and a `SAFETY` comment per block. Nothing else in the crate uses `unsafe`.
- Threads (ADR 0008): `PluginInstance` calls only inside main-thread jobs (`main_thread::run_blocking`, `MainThread::spawn`); `PluginProcessor` and the adapters' `process`/`set_parameter`/`reset` are RT-safe on our side (no allocation, lock, I/O, log, panic). `scan_file` runs only in `opendeck-plugin-scan`.
- Tests run against the `fake` backend with `ThreadMainThread`; RT paths under `assert_no_alloc`. Real plugin tests are `#[ignore]` and take paths from env vars (`OPENDECK_TEST_CLAP`, `OPENDECK_TEST_VST3`).
- No plugin code from GPL hosts (Ardour, Carla, Mixxx, Qtractor). The CLAP and VST3 specs and the MIT SDK are the sources.

## Skeleton (wave 9a, done)
- `PluginHost` (catalog, factories, `create_effect`/`create_instrument` with `InstanceHandle`), adapters `PluginEffect`/`PluginInstrument`, `MainThread` + `ThreadMainThread`, thread-local instance table, `backend` seam, `fake` backend, `scan` (folders, candidates, `Scanner`, `ScanRecord`, `scan_in_process`), scanner binary, CLAP scan listing, VST3 stub.

## Acceptance per wave
- **9a-1 scanner + CLAP effects in slots.** `ClapBackend::instantiate` with clack-host: `PluginInstance` (main thread), `StartedPluginAudioProcessor` as `PluginProcessor`, extensions `params` (descriptors, plain <-> normalized), `latency`, `state`, `audio-ports` (stereo main in/out required, else not listed). Param events in/out mapped to `ParamChange`; transport from `ProcessContext`. Host callbacks (`request_restart`, `request_callback`, `latency.changed`) queued to the main thread. app-core: scan job over default + user folders, `ScanRecord`s in library settings, rescan only changed files, blacklist view data, register factories, session marker for unclean exits. src-tauri: `MainThread` via `run_on_main_thread`; bundle `opendeck-plugin-scan` next to the app. Tests: fake host suite, an `#[ignore]` real-plugin test, `assert_no_alloc` around CLAP process with a CLAP test plugin built from clack's examples (MIT/Apache) if it builds in CI. Destroy instances whose instantiate timed out. CPU figure for 6 slots of a CLAP gain.
- **9a-2 VST3 effects.** Scan via `GetPluginFactory`/`IPluginFactory2/3`, kind from sub-categories, ids as 32 hex digits. `IComponent` + `IAudioProcessor` (setupProcessing, setActive, setProcessing, process with `IParameterChanges` in/out), `IEditController` on the main thread with `IComponentHandler` (performEdit to the param ring, restartComponent). State: component then controller, length-prefixed. Windows CI runs the ignored test against the SDK's `again` sample if built there.
- **9a-3 editor windows.** `open_editor(parent)`/`close_editor`/resize on `PluginInstance`; CLAP `gui` (embedded first, floating fallback), VST3 `IPlugView` + `IPlugFrame`. src-tauri creates a child window per editor and passes `raw-window-handle` parents: Win32 `HWND`, Cocoa `NSView`, X11 window (XWayland). Closing the window or removing the slot closes the editor first. Manual test on Windows and macOS in `docs/hardware-test-checklist.md`.
- **9a-4 instruments in sampler/pads.** Needs the engine contract additions in ADR 0008 (owner approval). Instrument slots (2) on the sampler bus, `install_instrument`, `InstrumentNote` command; pad assignment "slot N, key K"; MIDI keyboard routing (midi crate); "Render to sampler slot". Tests: fake sine through the engine offline renderer, note timing sample-accurate, no allocation.
- **9a-5 UI.** Plugin browser (search, format, vendor, kind, categories), FX slot picker showing built-ins then plugins, instrument slot picker, scan settings (folders, rescan, progress), blacklist with reason and retry, generic parameter view with plugin value text, "open editor" button. IPC DTOs from contracts `plugin` module (`specta`).

## Open questions
- Owner: in-process processing (recommended) vs out of process; instrument slot count; VST logo; Linux scope (ADR 0008).
- Scan timeout of 30 s is a guess (`TODO(verify)` with large sample-library plugins).
- Velocity for FLX4 pads is fixed at 1.0; other controllers may send velocity (`TODO(verify)`).
