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
- Feature `clap` (default since wave 9a-1): `clack-host` 0.2 and `clack-extensions` 0.2 (params, state, latency, tail, audio-ports; both MIT OR Apache-2.0), `clap-sys` 0.5 (MIT OR Apache-2.0); `libloading` ISC comes with clack-host. The CLAP spec and headers are MIT (`THIRD_PARTY_NOTICES.md`).
- Feature `vst3`: `vst3` 0.3 (MIT OR Apache-2.0, bindings from the MIT VST 3 SDK headers; ship Steinberg's MIT notice).
- Rejected: `vst3-sys` (GPL-3.0). Any new dependency: record the licence, run `cargo deny --all-features check licenses`.

## Rules
- Format modules (`clap*`, `vst3*`) may use `unsafe` with a module-level `allow(unsafe_code)` and a `SAFETY` comment per block. Nothing else in the crate uses `unsafe`.
- Threads (ADR 0008): `PluginInstance` calls only inside main-thread jobs (`main_thread::run_blocking`, `MainThread::spawn`); `PluginProcessor` and the adapters' `process`/`set_parameter`/`reset` are RT-safe on our side (no allocation, lock, I/O, log, panic). `scan_file` runs only in `opendeck-plugin-scan`.
- Tests run against the `fake` backend with `ThreadMainThread`; RT paths under `assert_no_alloc`. Real plugin tests are `#[ignore]` and take paths from env vars (`OPENDECK_TEST_CLAP`, `OPENDECK_TEST_VST3`).
- No plugin code from GPL hosts (Ardour, Carla, Mixxx, Qtractor). The CLAP and VST3 specs and the MIT SDK are the sources.

## Skeleton (wave 9a, done)
- `PluginHost` (catalog, factories, `create_effect`/`create_instrument` with `InstanceHandle`), adapters `PluginEffect`/`PluginInstrument`, `MainThread` + `ThreadMainThread`, thread-local instance table, `backend` seam, `fake` backend, `scan` (folders, candidates, `Scanner`, `ScanRecord`, `scan_in_process`), scanner binary, CLAP scan listing, VST3 stub.

## Wave 9a-1 (done: scanner + CLAP effects)
- `clap.rs` (backend, scan listing) with `clap/{host,instance,processor,params}.rs`. `ClapInstance` (main thread) holds `clack_host::plugin::PluginInstance`; `ClapProcessor` wraps the started audio processor. Activation: `min_frames` 1, `max_frames` block size, `start_processing` right after activation (control thread, before the hand-off). Main ports must be stereo (flag `IS_MAIN`, else port 0); other ports get silent input and discarded output. Parameters: hidden and read-only ones are not listed or settable but their output events still reach the main thread. Stepped params become `Stepped { steps }` only for integer ranges; everything else is `Linear` (CLAP has no curve info). Units are `None` (value text is wave 9a-5).
- Param changes keep their frame offset: `set_parameter` queues at frame 0, `InstanceHandle::send_param` (editor, automation, MIDI learn) carries an offset and lands on that frame as a `CLAP_EVENT_PARAM_VALUE` event. `ProcessBlock::params` is clamped to the block and sorted per block without allocating.
- `Effect::prepare` also refreshes `EffectInfo::has_tail` from the `tail` extension (zero tail: false; no extension: true).
- Host callbacks only set flags and call `HostWake` (queues one main-thread job); `PluginInstance::service` runs `on_main_thread` and `InstanceHandle::take_requests` returns `{restart, latency_changed, params_changed, state_dirty}`. app-core does not act on them yet (no automatic re-install on `latency_changed`).
- Instantiation that times out leaves a cleanup job behind the instantiate job that destroys the instance (`host.rs`).
- `scan_cache.rs`: `ScanCache` (JSON), `rescan` (reuse by path + size + mtime, blacklisted files stay blacklisted, `retry` and `all`, dropped files), `catalog()` (higher version wins, then earlier folder), `param_table()`. The scanner also records each plugin's parameters (`ScannedParams`) so factories list parameters without instantiating.
- `PluginEffectFactory` (`host.rs`) is what app-core registers: provider `clap`, keys `clap:<plugin id>`.
- app-core (`plugins.rs`): `PluginService` (host, cache in library settings `plugins.scan`, user folders `plugins.folders`, scan thread, catalog version the control thread follows). FX slots keep values by position and translate to the effect's own `ParamId` (`FxController::param_id` / `param_index`); built-ins are unchanged because their ids equal their positions.
- src-tauri: `TauriMainThread` (`run_on_main_thread`), commands `plugins_get_state`, `plugins_rescan`, `plugins_retry`, `plugins_set_folders`, event `plugins-changed`. Startup rescans new and changed files.
- Tests: `tests/clap_fixture.rs` against `tests/fixtures/clap-test-plugin` (a `clap-sys` cdylib, workspace member, copied to `gain.clap`, `crash.clap`, `hang.clap`; behaviour chosen by the file stem passed to `clap_entry.init`). Covers scan, subprocess blacklist (crash, hang), cache reuse and retry, gain, mid-block parameter events, no allocation in `process` (`assert_no_alloc`), state round trip, latency 64, tail 4800, reset, plugin output events, re-prepare, registry keys. `OPENDECK_TEST_CLAP=/path/Effect.clap cargo test -p opendeck-plugin-host -- --ignored real_clap` runs a real plugin. No real CLAP plugin was available, so third-party behaviour is untested (`docs/hardware-test-checklist.md`, M9a).
- Left for later: bundling `opendeck-plugin-scan` with the app (Tauri `externalBin` fails the build when the sidecar file is missing, so it needs a pre-build step; today it must sit next to the executable), the session marker for unclean exits, acting on `take_requests`, CPU figure for 6 slots, `has_editor` (wave 9a-3), a note-port fixture for instruments (CLAP notes are mapped but untested).

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
