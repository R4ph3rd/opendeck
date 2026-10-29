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

## External crates (add when used)
- `midir` 0.11, `serde`, `serde_json`, `thiserror`. Dev: `serde_json` (in use).

## Sources and rules (ADR 0007)
- Only byte source for the FLX4: `docs/sources/ddj-flx4-midi.txt` (git-ignored, read by absolute path). Cite section numbers (e.g. 1-4, 3-7, 5-5) in the mapping `label` or commit message.
- Unconfirmed values carry `"verify"` in the mapping and a row in `docs/midi-open-questions.md`.
- Relative encodings: jog `offset64`, browse `twos_complement`. 14-bit controls as `cc14` MSB/LSB.
- The midir input callback only decodes and pushes to a channel; no blocking, no locks held across sends.
- Every mapping feature gets a replay test (recorded bytes in, expected `ControlEvent` JSON out).

## Acceptance per milestone
- M6: FLX4 mapping covering sections 1 to 5 of the message list (decks, effect, mixer, browse, performance pads in all 8 modes, SHIFT variants); LED feedback for play, cue, sync, pad modes, pads, PFL, FX on, level meters, loaded and vinyl mode; monitor and learn backend; overrides from the DB applied on load; replay tests for every control group; `verify` count reported.

## Open questions
- Jog ticks per revolution (scale for `jog_top`/`jog_side`): not in the message list, measure.
- 14-bit MSB/LSB send order (list does not state it).
- Level meter value-to-dB curve for CH LEVEL METER (3-15).
- Pad LED colours: the list shows only on/off values for pads.
- BEAT SYNC long press (1-10, note 0x5C) meaning: map to sync leader or tempo-only sync.
