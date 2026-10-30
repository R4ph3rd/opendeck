# app-core

Owner: architect for structure; ui-shell-dev and feature owners add wiring per milestone with architect review.

## Purpose
Top of the crate graph. Owns application state (decks, mixer snapshot, pad modes, FX state, settings), runs the control thread, turns IPC calls and MIDI `ControlEvent`s into `EngineCommand`s, interprets pads per active `PadMode`, persists engine events (hot cue set, loop set) through the library, computes LED feedback, and forwards telemetry to the shell.

## Owned paths
- `crates/app-core/**`

## Depends on
- All crates: contracts, media, engine, dsp-fx, analysis, library, midi (ADR 0002).

## External crates (add when used)
- `crossbeam-channel` (control thread inbox), `tracing`, `thiserror`, `serde_json`.

## Interfaces it provides to `src-tauri`
- `AppCore` (exists: `pad_mode`, `set_pad_mode`, `audio_hosts` stub). Becomes a handle that sends messages to the control thread in M1; methods stay synchronous from the shell's view or return futures, decided in M1.
- A telemetry subscription the shell turns into `engine-telemetry` events at ~60 Hz.
- An `EffectRegistry` with the `"builtin"` factory registered.

## Rules
- MIDI: the bridge never rewrites the FX model's channel assignment; controls apply to the units that own the selected channels. Soft takeover for the tempo fader lives in the bridge. Sampler, keyboard and key shift pads and MASTER CUE are dropped until M7.
- Never runs on the audio thread; never holds a lock while waiting on a ring.
- Knob and fader curves (EQ, trim, filter, channel fader) live here or in the engine, not in the MIDI layer.
- Pad presses: `ControlAction::Pad { mode, .. }` is ignored if `mode` is not the deck's active mode.

## Acceptance per milestone
- M0: pad mode per deck and audio host stub. **Done in wave 0a.**
- M1: control thread, engine start/stop with settings, deck/mixer commands, telemetry forwarding.
- M2: library handle, import job events. M3: sync and analysis wiring (**done**: `LibraryService` owns the library and import queue, `load_track` by id with grid and cues, cue and settings persistence, session saved set).  M4: cue/loop persistence, pad interpretation for HOT CUE, BEAT JUMP, BEAT LOOP. M5: FX registry and state. M6: MIDI connection, feedback loop (**done** in wave 6b: `MidiBridge` translates `ControlEvent`s and derives LED states, `MidiService` runs it with the controller worker and the user's mapping overrides; see ADR 0007 addenda). M7: sampler, KEYBOARD, KEY SHIFT, saved sets.

## Open questions
- Undo for destructive edits (cue delete, grid edit): scope and storage.
- Whether app-core state snapshots are pushed to the UI as full DTOs or patches (start with full DTOs).
