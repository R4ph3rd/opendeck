# Status

Current milestone: **3**. Milestone 2 landed as v0.2.0 on 2026-10-29 (milestone 1 as v0.1.0, milestone 0 as v0.0.0).

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
| M3 app-core | `load_track` takes a path until `TrackId` lands |
| M3 app-core | Hot cues and memory cues live in memory only; persistence lands with the library |
| M3 app-core | No beat grids until analysis, so sync, auto loop, roll and beat jump do nothing on real tracks; no Tauri command for `set_beat_grid` yet |
| M4 ui | No jog wheel and no manual BPM entry in the UI |
| M4 ui | Two-deck layout not visually checked on a real screen |
| M6 engine | Jog nudge sensitivity, `MAX_PITCH_BEND` and the linear channel fader law are product guesses, `TODO(verify)` |
| M8 engine | A quantized hot cue waits for the next beat; confirm against user tests |
| M8 engine | Heap-count test (`tests_heap`) runs on Linux only |
| M3 library | `playlist_tracks` reorder hits `UNIQUE(playlist_id, position)` on bulk shift |
| M3 library | Folder-guard trigger only fires on INSERT |
| M6 midi | `on_connect` raw messages are capped at 3 bytes, so no SysEx |
| M6 midi | Feedback entries lack `when` for shift-layer LEDs |
| M6 midi | Drift test only checks schema to contracts |

## Doc follow-ups

- Agent definitions in `.claude/agents` load only in new Claude Code sessions.
