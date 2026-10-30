# Status

Current milestone: **4 (next)**. Milestone 3 landed as v0.3.0 on 2026-10-30 (milestone 2 as v0.2.0, milestone 1 as v0.1.0, milestone 0 as v0.0.0).

## Milestones

| M | Scope | State |
|---|---|---|
| 0 | Architecture, contracts, skeleton, CI, design tokens | done (v0.0.0) |
| 1 | Audio engine: one deck plays a file, device selection, offline render tests | done (v0.1.0) |
| 2 | Two decks, mixer, routing with cue, sync, loops, hot cues | done (v0.2.0) |
| 3 | Library, DB, import pipeline, analysis, collection view | next |
| 4 | Waveforms and deck UI, design system components in Storybook | planned |
| 5 | FX engine and FX UI with presets and chains | planned |
| 6 | MIDI layer, DDJ-FLX4 mapping, LED feedback, MIDI monitor, replay harness | planned |
| 7 | Pad modes with the dynamic middle panel, sampler | planned |
| 8 | Hardening: latency, error handling, settings, installer, first-run | planned |
| 9 | Post-MVP: VST3/CLAP, 4 decks, recording, more controllers (needs owner confirmation) | not scheduled |

## Milestone 0

Done: ADR 0001 to 0007, cargo workspace, contracts crate, DB schema v1, Tauri shell, typed IPC bindings, CI and release workflows, design tokens, Storybook, stylelint and ESLint token rules, docs. Released as v0.0.0.

## Milestone 1

Done: engine with one deck playing a `TrackBuffer` (play/pause, CDJ CUE, seek, tempo ranges, pitch bend, key lock with amortised priming, click-free jumps), cpal output with device, rate and buffer selection, 4-channel routing with stereo fallback, telemetry, `OfflineRenderer`, zero heap calls in the callback (Rust and C++). Media decode to engine rate. Device lost, changed and error events. App-core control and loader threads with audio settings. Tauri audio and transport commands with engine events. UI: app layout, deck A panel, audio settings, toasts, keyboard shortcuts.

## Milestone 2

Done: two decks with jog scratch and nudge, tempo and beat sync with leader election, quantize, loops with click-free wraps, loop roll, 8 hot cues, beat jump and slip. Mixer with trim, 3-band isolator EQ (LR8, kill), one-knob filter, channel fader, PFL, crossfader with three curves, master level and a headphone bus with cue/master blend and split cue on the headphone pair. Tracks carry an optional beat grid (`load_track_with_grid`, `set_beat_grid`). Telemetry reports channel, master and headphone meters, beat phase, sync mode, leader and loop state per deck. App-core deck, mixer, loop and cue control, hot cues re-sent after every load. Tauri commands for all of it. UI: deck B, mixer strip, loops, hot cue pads, sync/master/quantize/slip, two-deck shortcuts; design system Toaster and slider detent. Offline tests through the engine and through app-core prove EQ kill, crossfader, PFL, loops and beat sync change the audio.

## Branch model

- `main`: released milestones only. Every push to `main` runs the release workflow.
- `workbench`: integration branch. All work lands here after the checker gate.
- `feat/*`: one branch per task, opened from `workbench`, merged back by pull request.
- Checker gate: fmt, clippy `-D warnings`, `cargo test --workspace`, bindings diff, UI typecheck/lint/test, Storybook build, `cargo deny check`. CI runs the same steps.
- Promotion: when a milestone passes the gate, `workbench` merges into `main` (see `docs/RELEASING.md`).

## Carried-over issues

| Milestone | Issue |
|---|---|
| M8 engine | No auto-reopen after device loss; the user reselects the device |
| M8 app-core | Freed track buffers are dropped on the control thread, not the loader thread as ADR 0003 says |
| M8 app-core | Playhead is not restored after an engine restart |
| M3 app-core | Memory cues have no delete or update command, and cue names and colours have no edit command (M4) |
| M3 app-core | `nudge_beat_grid` is not implemented; `set_beat_grid` stores a locked grid but no UI edits it yet |
| M3 app-core | The collection comment column costs one `get_track` per row of a page, because the library row type has no comment |
| M3 ui | Collection, sidebar and import panel are covered by jsdom tests only; not checked in a real Tauri window |
| M3 ui | OS file drops are off (`dragDropEnabled: false`, needed for in-app drag and drop on Windows); files come in through the Files tab or the Import menu |
| M3 ui | Ctrl+A on a very large collection fetches every page before selecting |
| M3 ui | No tag editor in the UI; `update_track_meta` exists |
| M4 ui | No jog wheel and no manual BPM entry in the UI |
| M4 ui | Two-deck layout not visually checked on a real screen |
| M6 engine | Jog nudge sensitivity, `MAX_PITCH_BEND` and the linear channel fader law are product guesses, `TODO(verify)` |
| M8 engine | A quantized hot cue waits for the next beat; confirm against user tests |
| M8 engine | Heap-count test (`tests_heap`) runs on Linux only |
| M6 midi | `on_connect` raw messages are capped at 3 bytes, so no SysEx |
| M6 midi | Feedback entries lack `when` for shift-layer LEDs |
| M6 midi | Drift test only checks schema to contracts |

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
