# Status

Current milestone: **0 (in progress)**.

## Milestones

| M | Scope | State |
|---|---|---|
| 0 | Architecture, contracts, skeleton, CI, design tokens | in progress |
| 1 | Audio engine: one deck plays a file, device selection, offline render tests | planned |
| 2 | Two decks, mixer, routing with cue, sync, loops, hot cues | planned |
| 3 | Library, DB, import pipeline, analysis, collection view | planned |
| 4 | Waveforms and deck UI, design system components in Storybook | planned |
| 5 | FX engine and FX UI with presets and chains | planned |
| 6 | MIDI layer, DDJ-FLX4 mapping, LED feedback, MIDI monitor, replay harness | planned |
| 7 | Pad modes with the dynamic middle panel, sampler | planned |
| 8 | Hardening: latency, error handling, settings, installer, first-run | planned |
| 9 | Post-MVP: VST3/CLAP, 4 decks, recording, more controllers (needs owner confirmation) | not scheduled |

## Milestone 0

Done: ADR 0001 to 0007, cargo workspace, contracts crate, DB schema v1, Tauri shell, typed IPC bindings.

Remaining: CI and release workflows, design tokens, Storybook, stylelint, docs.

## Branch model

- `main`: released milestones only. Every push to `main` runs the release workflow.
- `workbench`: integration branch. All work lands here after the checker gate.
- `feat/*`: one branch per task, opened from `workbench`, merged back by pull request.
- Checker gate: fmt, clippy `-D warnings`, `cargo test --workspace`, bindings diff, UI typecheck/lint/test, Storybook build, `cargo deny check`. CI runs the same steps.
- Promotion: when a milestone passes the gate, `workbench` merges into `main` (see `docs/RELEASING.md`).

## Carried-over issues

| Milestone | Issue |
|---|---|
| M3 library | `playlist_tracks` reorder hits `UNIQUE(playlist_id, position)` on bulk shift |
| M3 library | Folder-guard trigger only fires on INSERT |
| M6 midi | `on_connect` raw messages are capped at 3 bytes, so no SysEx |
| M6 midi | Feedback entries lack `when` for shift-layer LEDs |
| M6 midi | Drift test only checks schema to contracts |
| M1 engine | `EngineConfig.sample_rate` is `u32`, not the `SampleRate` newtype |

## Doc follow-ups

- Agent definitions in `.claude/agents` load only in new Claude Code sessions.
