# midi

Owner: midi-dev.

## Purpose
Generic MIDI layer (port discovery, input and output), data-driven JSON mappings that turn MIDI into `ControlEvent`s and `FeedbackTarget` states into MIDI out (LEDs, meters), MIDI monitor and learn backend, replay harness. First mapping: Pioneer DJ DDJ-FLX4.

## Owned paths
- `crates/midi/**`
- `mappings/**` (the schema in `mappings/schema/` changes only with the architect)
- `docs/midi-open-questions.md`

## Depends on
- `opendeck-contracts` (`serde` feature): `ControlAction`, `ControlValue`, `ControlEvent`, `FeedbackTarget`, `FeedbackValue`, `PadMode`, ids. Never the engine.

## External crates
- `midir` 0.11 (MIT), `crossbeam-channel`, `serde`, `serde_json`, `thiserror`.

## Sources and rules (ADR 0007)
- Only byte source for the FLX4: `docs/sources/ddj-flx4-midi.txt` (git-ignored, read by absolute path). Cite section numbers (e.g. 1-4, 3-7, 5-5) in the mapping `label` or commit message.
- Unconfirmed values carry `"verify"` in the mapping and a row in `docs/midi-open-questions.md`.
- Relative encodings: jog `offset64`, browse `twos_complement`. 14-bit controls as `cc14` MSB/LSB.
- The midir input callback only decodes and pushes to a channel; no blocking, no locks held across sends.
- Every mapping feature gets a replay test (recorded bytes in, expected `ControlEvent` JSON out).
- `on_connect` takes 3-byte messages or a complete SysEx; `feedback` entries take `when` (ADR 0007, wave 6a addendum).

## Runtime (M6, ADR 0007 addenda)
- `Pipeline` = `MappingEngine` + `FeedbackEngine`, driven with explicit timestamps; the replay harness and the `Controller` worker run the same code.
- `Controller`: one worker thread, 1 s port rescan, hot-plug, `on_connect` and LED replay on every output connect. The midir callback copies at most 3 bytes into a bounded channel with `try_send`.
- app-core owns `MidiBridge` and `MidiService` (its own thread): actions, soft takeover, LED state, monitor and learn for the UI. The engine never depends on this crate.
- User overrides are JSON in the library settings kv (`midi.settings`). The `midi_mapping_overrides` table (ADR 0005) is unused: the library crate has no API for it.

## Acceptance per milestone
- M6 (done): FLX4 mapping covering sections 1 to 5 of the message list (decks, effect, mixer, browse, performance pads in all 8 modes, SHIFT variants); LED feedback for play, cue, sync, pad modes, pads, PFL, FX on, level meters, loaded and vinyl mode; monitor and learn backend; overrides applied on load; replay tests for every control group; 165 `verify` markers over 9 questions.

## Open questions
Tracked in `docs/midi-open-questions.md` (Q02 jog scale, Q03 14-bit order, Q04 meter curve, Q07 to Q14). Q01 and Q15 are resolved. Hardware steps are in `docs/hardware-test-checklist.md` (M6).
