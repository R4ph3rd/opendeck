# Status

Current milestone: **8 (next)**. Milestone 7 landed as v0.7.0 on 2026-10-31 (milestone 6 as v0.6.0, milestone 5 as v0.5.0, milestone 4 as v0.4.0, milestone 3 as v0.3.0, milestone 2 as v0.2.0, milestone 1 as v0.1.0, milestone 0 as v0.0.0).

## Milestones

| M | Scope | State |
|---|---|---|
| 0 | Architecture, contracts, skeleton, CI, design tokens | done (v0.0.0) |
| 1 | Audio engine: one deck plays a file, device selection, offline render tests | done (v0.1.0) |
| 2 | Two decks, mixer, routing with cue, sync, loops, hot cues | done (v0.2.0) |
| 3 | Library, DB, import pipeline, analysis, collection view | done (v0.3.0) |
| 4 | Waveforms and deck UI, design system components in Storybook | done (v0.4.0) |
| 5 | FX engine and FX UI with presets and chains | done (v0.5.0) |
| 6 | MIDI layer, DDJ-FLX4 mapping, LED feedback, MIDI monitor, replay harness | done (v0.6.0) |
| 7 | Pad modes with the dynamic middle panel, sampler | done (v0.7.0) |
| 8 | Hardening: latency, error handling, settings, installer, first-run (MVP) | done |
| 9 | Post-MVP: VST3/CLAP, 4 decks, recording, more controllers (needs owner confirmation) | not scheduled |

## Milestone 0

Done: ADR 0001 to 0007, cargo workspace, contracts crate, DB schema v1, Tauri shell, typed IPC bindings, CI and release workflows, design tokens, Storybook, stylelint and ESLint token rules, docs. Released as v0.0.0.

## Milestone 1

Done: engine with one deck playing a `TrackBuffer` (play/pause, CDJ CUE, seek, tempo ranges, pitch bend, key lock with amortised priming, click-free jumps), cpal output with device, rate and buffer selection, 4-channel routing with stereo fallback, telemetry, `OfflineRenderer`, zero heap calls in the callback (Rust and C++). Media decode to engine rate. Device lost, changed and error events. App-core control and loader threads with audio settings. Tauri audio and transport commands with engine events. UI: app layout, deck A panel, audio settings, toasts, keyboard shortcuts.

## Milestone 2

Done: two decks with jog scratch and nudge, tempo and beat sync with leader election, quantize, loops with click-free wraps, loop roll, 8 hot cues, beat jump and slip. Mixer with trim, 3-band isolator EQ (LR8, kill), one-knob filter, channel fader, PFL, crossfader with three curves, master level and a headphone bus with cue/master blend and split cue on the headphone pair. Tracks carry an optional beat grid (`load_track_with_grid`, `set_beat_grid`). Telemetry reports channel, master and headphone meters, beat phase, sync mode, leader and loop state per deck. App-core deck, mixer, loop and cue control, hot cues re-sent after every load. Tauri commands for all of it. UI: deck B, mixer strip, loops, hot cue pads, sync/master/quantize/slip, two-deck shortcuts; design system Toaster and slider detent. Offline tests through the engine and through app-core prove EQ kill, crossfader, PFL, loops and beat sync change the audio.

## Milestone 4

Done: zoomed and overview waveforms from analysis peaks (`get_track_peaks`), with beat grid, hot and memory cues, loops and a playhead extrapolated from telemetry; zoom (2 to 64 s) and scrub. Canvas knob, jog wheel (touch, turn, nudge ring, Shift search), performance pads and stereo level meter, wired into the deck and mixer panels. Design system: info, blink and sweep tokens with contrast tests; resizable, popover, command, number field, status dot, segmented control, settings layout, breadcrumb, callout, key-value, skeleton, progress bar and colour swatch picker, with stories and pattern docs. Flat FL-style slate re-theme: gradients, mesh backgrounds and backdrop blur removed and banned in stylelint and ESLint. Viz Storybook (`pnpm -C ui storybook:viz`) with a two-deck and mixer layout story on synthetic tracks.

## Milestone 5

Done: 17 built-in effects (`dsp-fx`) with catalog data, factory presets and a reference slot host. Engine FX host: `MAX_DECKS` deck units, a master unit and two send units, each a chain of `FX_SLOTS_PER_UNIT` (6) slots, with equal-power mix, 10 ms bypass, tail spill, slot latency compensation, sample-accurate timed engage, Release FX, tapped tempo source, per-unit telemetry and zero heap calls in the callback. App-core FX model: two FLX4-style Beat FX units (channel 1, 2 or 1&2, Multi and Single, BEAT stepper, tap BPM, Release FX), Sound Color FX per channel, two send buses (pre or post fader), 32 pad FX, user presets in `fx_presets`, chain presets and persisted state. Each deck chain has dedicated slots: 0 to 2 Beat FX, 3 Sound Color FX, 4 pad FX, 5 Release FX, so a full Multi unit no longer disables the others. Tauri FX commands and `fx-state-changed`. UI: effects panel with Beat FX units, CFX knobs and sends on the channel strips, send units, pad FX editor, presets and chains.

## Milestone 6

Done: `midi` crate with data-driven JSON mappings (ADR 0007 and addenda): input engine with 14-bit pairs, relative encodings and shift layers; feedback engine with per-address de-duplication, rate limits, software blink and `when`; `on_connect` with SysEx; hot-plug controller worker with a lock-free midir callback; monitor stream and learn backend; replay harness. DDJ-FLX4 mapping for message list sections 1 to 5 with cited rows, unmapped rows listed with a reason, and LED feedback for play, cue, sync, loops, pads, pad modes, PFL, FX on, level meters, loaded and vinyl mode. Five additive contract actions (memory cue delete, tap tempo, FX release, Beat FX BPM auto and tap). App-core `MidiBridge` and `MidiService`: actions, Beat FX section routing, tempo soft takeover, CFX knob, fader start setting, overrides, LED state from telemetry, headless FLX4 replay through app-core. Tauri MIDI commands and events. UI: MIDI settings (device, auto-connect, fader start, mapping table, learn, overrides, import and export), MIDI monitor and a controller status indicator. MIDI overrides are stored as JSON in the settings kv (`midi.settings`); the `midi_mapping_overrides` table has no library API and stays unused.

## Milestone 7

Done: the pad mode drives the middle panel of each deck, with a view for each of the 8 modes (HOT CUE with cue rename, PAD FX 1 and 2, BEAT JUMP, SAMPLER, KEYBOARD, BEAT LOOP, KEY SHIFT). App-core interprets pads in all 8 modes with their SHIFT layers through `PadActions`, for the UI and the FLX4 alike, and drives the pad LEDs. Engine sampler: 16 slots on a sampler bus before the master FX, one-shot, gate and loop modes, quantized starts on the master beat, per-slot gain and varispeed sync, bus level, PFL, slot and bus telemetry, zero heap calls in the callback. Key shift of +/- 12 semitones through the stretcher with key lock on or off. App-core sampler service: slot assignment, split or shared banks, decode to the engine rate, persistence in the library settings, hand-off to the engine (`EngineSampler`) with replay after an engine restart. KEYBOARD plays the keyboard cue chromatically; BEAT LOOP sets and exits auto loops. Tauri pad, sampler and hot cue rename commands and events. UI: middle panel with mode views, sampler slot cards (drop, name, gain, mode, sync, colour, play position) and the sampler bus meter.

## Milestone 8

Done (MVP): output routing of master, headphones and booth to any channel pair of one device (up to 32 channels), mono per output, booth level, stereo fallback. Mic and aux inputs on a second stream with an input ring, mic level, two-shelf EQ and on/off, aux level and fader, talk-over ducking. Device loss reopens with backoff and restores loaded tracks, playheads, play state and cue points; a lost input reattaches without touching the decks; returned buffers are freed on the loader thread. Measured or estimated latency, xrun counter and capability flags per host. Test tone through the engine on each output, so it works while the engine holds the device. Analyzer BPM range from Settings, Library. Log files with rotation, panic hook, database check with backup and recovery, typed error messages, error boundary, diagnostics copy. Settings pages: Audio (device, routing, inputs, talk-over, test tones), MIDI, Library, Interface (theme, waveform zoom, layout presets), Keyboard (shortcut editor) and Advanced (logs, diagnostics, reset). First-run guide (audio with FLX4 preselect, controller check, library, tour). Installer config: product metadata, deb desktop entry, NSIS per-user install, fixed WiX upgrade code (`docs/RELEASING.md`).

## Post-MVP

| Area | Item |
|---|---|
| engine | Separate output device for the headphones (two clocks need async resampling) |
| engine | Drift correction for an input on a different device than the output (resampling with `rubato`); today the input ring cuts or inserts silence |
| engine | Input resampling when the input device cannot run at the output rate |
| engine | WASAPI exclusive mode |
| engine | JACK host: the `jack` feature was never built or run here (needs libjack at build time) |
| ui | Mic and aux controls live in Settings, Audio; no mic or aux strip on the mixer |
| tauri | File associations (open audio files and playlists from the OS) |
| all | 4 decks in the UI (engine and contracts are ready for `MAX_DECKS`) |
| M9 | VST3 and CLAP plugin hosting (ADR 0006), owner-approved |

## Branch model

- `main`: released milestones only. Every push to `main` runs the release workflow.
- `workbench`: integration branch. All work lands here after the checker gate.
- `feat/*`: one branch per task, opened from `workbench`, merged back by pull request.
- Checker gate: fmt, clippy `-D warnings`, `cargo test --workspace`, bindings diff, UI typecheck/lint/test, Storybook build, `cargo deny check`. CI runs the same steps.
- Promotion: when a milestone passes the gate, `workbench` merges into `main` (see `docs/RELEASING.md`).

## Carried-over issues

| Milestone | Issue |
|---|---|
| M3 app-core | Memory cues have no delete or update command, and cue names and colours have no edit command (M4) |
| M3 app-core | `nudge_beat_grid` is not implemented; `set_beat_grid` stores a locked grid but no UI edits it yet |
| M3 app-core | The collection comment column costs one `get_track` per row of a page, because the library row type has no comment |
| M3 ui | Collection, sidebar and import panel are covered by jsdom tests only; not checked in a real Tauri window |
| M3 ui | OS file drops are off (`dragDropEnabled: false`, needed for in-app drag and drop on Windows); files come in through the Files tab or the Import menu |
| M3 ui | Ctrl+A on a very large collection fetches every page before selecting |
| M3 ui | No tag editor in the UI; `update_track_meta` exists |
| M4 ui | No manual BPM entry in the UI |
| M4 ui | Two-deck layout checked in headless Chromium Storybook screenshots only; not in a real Tauri window |
| M4 ui | Waveforms are Canvas 2D only; the WebGL2 renderer in `CONTEXT/ui-viz.md` is not done |
| M4 ui | Waveform frame times not measured in a real browser; only stubbed-canvas JS cost (p95 at most 2.0 ms per deck). Use the `Viz/Perf` story |
| M4 ui | No global zoom shortcut; `+` and `-` work only while a waveform has focus |
| M4 ui | Waveform scrub sends `seek` once per frame; there is no jog-search scrub |
| M4 ui | Tracks loaded by path (not from the library) have no peaks, so no waveform |
| M4 ui | Jog Shift search uses `SEARCH_BEATS_PER_REV` = 16, a UI choice, `TODO(verify)` with the engine owner |
| M4 ui | Hot cue colours come from the `--cue-1..8` tokens by slot; `Cue.color` is ignored |
| M6 engine | Jog nudge sensitivity, `MAX_PITCH_BEND` and the linear channel fader law are product guesses, `TODO(verify)` |
| M8 engine | A quantized hot cue waits for the next beat; confirm against user tests |
| M8 engine | Heap-count test (`tests_heap`) runs on Linux only |
| M6 midi | 165 FLX4 mapping entries carry `verify` and depend on 9 open hardware questions (Q02, Q04, Q07 to Q13, `docs/midi-open-questions.md`) |
| M6 app-core | Placeholders, `TODO(verify)`: jog scale (1/1024 rev per tick, Q02), VU meter curve (36 dB range, Q04), beat jump distances and beat loop lengths per pad, SHIFT + jog search at 32 beats per revolution |
| M7 app-core | MASTER CUE is not in the mapping and stays unmapped |
| M7 app-core | KEYBOARD sets a sticky deck key shift (no per-trigger pitch): the shift stays after a pad release and after leaving KEYBOARD mode |
| M7 engine | Sampler sync is varispeed: a synced slot plays at master BPM / sample BPM and its pitch follows the tempo. Samples loaded by path have no BPM and do not sync |
| M7 midi | Pad modes, sampler banks (split or shared, Q05), keyboard pitches and key shift never run on a real DDJ-FLX4 (`docs/hardware-test-checklist.md`, M7) |
| M6 midi | Never run on a real DDJ-FLX4; covered by replay fixtures and a fake backend only (`docs/hardware-test-checklist.md`, M6) |
| M8 library | MIDI overrides live in the settings kv, not in the `midi_mapping_overrides` table (ADR 0005), which has no library API |
| M5 dsp-fx | Effect parameter names, ranges and defaults and the beat division list are `TODO(verify)` pending the rekordbox manual (`docs/fx-catalog.md`) |
| M5 app-core | Smart CFX mapping (knob to parameter and wet), the default pad FX set and the Release FX set are project choices, not taken from a source |
| M7 ui | Pad FX view has the assignment grid but no live parameter knobs (the pad model carries effect, division and release only) |

## Open questions

MIDI questions live in `docs/midi-open-questions.md`. Others:

| Area | Question | Status |
|---|---|---|
| M3 library | rekordbox XML `POSITION_MARK` `Type` codes as read (0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop) | TODO(verify) against the Pioneer rekordbox XML Format List |
| M3 library | rekordbox XML `POSITION_MARK` `Num` as read (-1 memory cue, 0 and up hot cue slot) | TODO(verify) |
| M3 library | rekordbox XML `Rating` scale as read (0 to 255 in steps of 51, mapped to 0 to 5) | TODO(verify) |
| M3 library | rekordbox XML `TEMPO` `Battito` as the beat number (1 to 4) of the marker in its bar | TODO(verify) |
| M3 library | Tracks with several `TEMPO` segments keep only the first until contracts has a tempo map (grid blob version 2) | known limit |

## Doc follow-ups

- Agent definitions in `.claude/agents` load only in new Claude Code sessions.
