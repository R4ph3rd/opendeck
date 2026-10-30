# ADR 0007: MIDI mapping file format

- Status: accepted
- Date: 2026-10-29
- Deciders: architect

## Context

Controller support must be data-driven: adding a controller or fixing a byte is a JSON change, not a code change. The first controller is the Pioneer DJ DDJ-FLX4; its only byte source is the "DDJ-FLX4 List of MIDI messages" E1 (`docs/sources/ddj-flx4-midi.txt`, git-ignored). Values not confirmed there must be visibly marked.

## Decision

Mappings are JSON files in `mappings/`, validated against `mappings/schema/mapping.schema.json` (JSON Schema 2020-12). The midi crate parses them with serde into its own types; actions and feedback targets reuse the contracts serde forms (`ControlAction`, `FeedbackTarget`), so a mapping can express exactly what app-core understands.

### File shape

```json
{
  "$schema": "schema/mapping.schema.json",
  "format_version": 1,
  "id": "pioneer-ddj-flx4",
  "version": 1,
  "name": "Pioneer DJ DDJ-FLX4",
  "source": "Pioneer DJ, DDJ-FLX4 List of MIDI messages, E1",
  "devices": { "input": ["DDJ-FLX4"], "output": ["DDJ-FLX4"] },
  "modifiers": [{ "id": "shift_deck1", "midi": { "type": "note", "channel": 1, "number": 63 } }],
  "controls": [
    { "id": "deck1.play", "label": "PLAY/PAUSE", "midi": { "type": "note", "channel": 1, "number": 11 },
      "action": { "action": "play_pause", "deck": 0 }, "value": { "kind": "button" } }
  ],
  "feedback": [
    { "target": { "target": "playing", "deck": 0 }, "midi": { "type": "note", "channel": 1, "number": 11 },
      "output": { "kind": "light", "on": 127, "off": 0 } }
  ]
}
```

(The example bytes are the message list's PLAY/PAUSE deck 1 row, 1-1: channel 1, note 0x0B, OFF=0x00, ON=0x7F; SHIFT 1-3 is note 0x3F.)

### Rules

- **Channels** are written 1-16 as printed in the message list; the parser subtracts one.
- **Unconfirmed values**: any modifier, control, feedback entry or raw message may carry `"verify": "<what to check>"`. This is the machine-readable form of `TODO(verify)`. The loader accepts them, the MIDI monitor highlights them, a test counts them, and each one is listed in `docs/midi-open-questions.md`. A value without a source and without `verify` is a review failure.
- **Shift layers**: generic `modifiers` (held buttons) plus a per-control `when` map (`{"shift_deck1": true}`). On the FLX4 most controls already send a different message with SHIFT held (message list column "+SHIFT"), so those are plain extra controls; `when` is for controllers that do not.
- **Pad modes**: on the FLX4 each pad sends a different note per pad mode (section 5-5: 0x00-0x07 HOT CUE, 0x10 PAD FX 1, 0x20 BEAT JUMP, 0x30 SAMPLER, 0x40 KEYBOARD, 0x50 PAD FX 2, 0x60 BEAT LOOP, 0x70 KEY SHIFT; channels 8/10 for decks 1/2, 9/11 with SHIFT). Each is a control with `{"action": "pad", "mode": ..., "index": ..., "shifted": ...}`. Mode buttons map to `pad_mode_select`; app-core owns the active mode.
- **Relative encoders**: `value.kind = "relative"` with `encoding`:
  - `offset64`: delta = v - 64. FLX4 jog (1-4): "clockwise increases from 0x41, counterclockwise decreases from 0x3F".
  - `twos_complement`: delta = v if v < 64 else v - 128. FLX4 BROWSE (4-1): "clockwise increases from 0x01, counterclockwise decreases from 0x7F".
  - `sign_magnitude`: bit 6 is the sign, bits 0-5 the magnitude (other controllers).
  - `scale` converts ticks to the action's unit (jog: revolutions). TODO(verify): FLX4 jog ticks per revolution is not in the message list; measure and record it with `verify` until confirmed.
- **14-bit faders and knobs**: `midi.type = "cc14"` with `msb` and `lsb` CC numbers, value = msb * 128 + lsb, normalized by 16383. The message list gives MSB/LSB pairs (e.g. TEMPO 1-11: CC 0x00 MSB, 0x20 LSB; CH FADER 3-7: CC 0x13 / 0x33; CROSSFADER 3-8: CC 0x1F / 0x3F on channel 7) and min/max 0x00/0x00 to 0x7F/0x7F. TODO(verify): the list does not state send order; the decoder stores the MSB and emits on each LSB, and also emits on an MSB that arrives alone after a timeout (default 5 ms).
- **LED feedback**: `feedback` entries map a `FeedbackTarget` to a note or CC with `light` (on/off values, optional blink) or `level` (meters) output. Blink is either `device` (send a value the device blinks on) or `timer` (software toggling). FLX4 note *4: FX ON/OFF blinks on NOTE ON and lights on NOTE OFF, which a `device` blink expresses. FLX4 CH LEVEL METER (3-15) is CC 0x02 on channels 1/2 with colour bands given in the list; the dB-to-value curve is TODO(verify).
- **Startup**: `on_connect` raw messages are sent when the output opens (e.g. vinyl mode 0x90/0x91 0x17, note *2: the FLX4 cannot change it itself).
- **User overrides** are not written into shipped files: they live in `midi_mapping_overrides` (ADR 0005) keyed by `(id, control id)` and are applied on load. If the base `version` changed since an override was made, the UI flags it.

### Replay harness

Recorded MIDI input (timestamped bytes) plus a mapping produce a `ControlEvent` stream; tests compare it to an expected JSON file. Every mapping feature gets a replay test (midi-dev).

## Consequences

- New controllers need no Rust changes unless they need a new encoding.
- Mapping files are verbose (one entry per message, including SHIFT and per-mode pad notes); a generator script is allowed but the committed JSON is the source of truth.
- `ControlAction` changes are contract changes and update the schema enum in the same commit.

## Addendum: M6 wave 6a, FLX4 mapping and runtime (2026-10-30)

Status: accepted. Resolves the three carried issues (SysEx in `on_connect`, `when` on feedback, one-way drift test) and records what the first full mapping settled.

### Format changes

All additive; `format_version` stays 1.

- `on_connect` messages may be longer than 3 bytes when they are a complete SysEx (`F0 ... F7`, 7-bit data). Anything else longer than 3 bytes is rejected on load.
- `feedback` entries take `when` with the same meaning as on controls. An entry with `when` is only sent while its modifiers match. When a modifier changes, the cached state of entries that just became active is sent (de-duplicated per address). The FLX4 mapping does not need it, because the controller has separate `+SHIFT` MIDI-OUT addresses that are fed the same state (open question Q13); it is for controllers whose LEDs change with the layer.
- `value.kind = "button"` takes `invert` (press reports `false`, release `true`). The FLX4 uses it for the fader-start CUE message (3-7).
- Every modifier, control and feedback entry may carry `ref`, the row or figure id in the source (`"1-2"`; `"illumination:LOADED"` and `"settings:VINYL MODE"` for the two tables without figure numbers). The FLX4 mapping cites one for every entry and a test enforces it. Feedback entries may also carry `label`.
- Root `unmapped`: source rows that are deliberately not mapped, with a reason (`ref`, `label`, `midi`, `reason`). The monitor labels such messages instead of calling them unknown.
- The `verify` text starts with a question id (`Q07`) that must have a `TODO(verify)` row in `docs/midi-open-questions.md`; a test checks it in both files and checks the counts stated in the doc.

### Drift tests

`crates/midi/tests/schema.rs` checks both directions. Every schema action or target is a contracts variant (serde probe). Every contracts variant is in the schema enum: a macro generates an exhaustive `match` and the name list from one input, so a new `ControlAction` or `FeedbackTarget` variant fails to compile until it is listed. Every action the FLX4 mapping uses is in the schema, and every variant it does not use is on a reviewed list with a reason (`NOT_ON_FLX4`). Property names in `ddj-flx4.json` must exist in the schema.

### Contract additions

Five additive `ControlAction` variants for FLX4 controls that had no action: `MemoryCueDelete` (SHIFT + CUE/LOOP CALL left, 1-8), `TapTempo` (SHIFT + CH CUE, 3-6), `FxRelease` (SHIFT + FX ON/OFF, 2-6), `FxBpmAuto` and `FxBpmTap` (SHIFT + BEAT left and right, 2-3 and 2-4). `FxAssign` documents `Button(bool)` as set or clear.

### Runtime

- `Pipeline` = `MappingEngine` (bytes to `ControlEvent`) + `FeedbackEngine` (state to bytes). Both are state machines driven with explicit microsecond timestamps; the replay harness and the worker thread run the same code.
- 14-bit controls: the engine emits on every LSB with the last MSB, and emits an MSB that stays alone for 5 ms with the last LSB. Equal consecutive values are dropped. Send order stays Q03.
- Feedback policy: per `(status, data1)` address, de-duplication and rate limiting (lights 1 ms, meters 33 ms). A newer value replaces a waiting one, so the final state always arrives. Timer blinks and waiting values are released by `tick`.
- `Controller` runs one worker thread. The midir callback copies at most 3 bytes and a timestamp into a bounded channel with `try_send` (longer messages and overflow are counted, never blocked on). The worker rescans ports every second, opens the ports the mapping's `devices` patterns match, closes them when the device vanishes, and on every output connect sends `on_connect` and replays the cached LED state. Driver timestamps are mapped onto the worker clock at the first message of a connection, keeping the driver's spacing.
- Monitor stream (`MonitorEvent`: timestamp, direction, bytes, text, label with the control id and its `verify`) and MIDI learn (`LearnSession`, `build_control`, `Mapping::upsert_control`) are library API; the shell wires them in a later wave.
- `Mapping::apply_input_overrides` applies `midi_mapping_overrides` rows (ADR 0005) and reports ids that no longer exist.
- Soft takeover is not applied here. The FLX4 sends no position dump on connect, so absolute controls have no value until they move. `MappingEngine::last_absolute` gives app-core the last reported position for its own pickup policy.

### FLX4 conventions for app-core

- **Edges**: toggles and one-shot buttons report presses only (`edge: press`): PLAY, SYNC, IN, OUT, 4 BEAT, CALL, LOAD, CH CUE, FX SELECT, BEAT, mode buttons. CUE, jog touch, pads, FX release and the FX CH SELECT flags report both edges.
- **Pads**: `Pad { mode, index, shifted }`; `shifted` is the channel 9/11 layer. The mode is the high nibble of the note, so app-core's active-mode filter drops presses of other modes.
- **Jog**: `jog_top` on both the vinyl-on (CC 0x22) and vinyl-off (CC 0x23) CCs, `jog_search` on SHIFT (CC 0x29), `jog_side` on CC 0x21. The scale is a placeholder (Q02). The vinyl state is app-core's; the device cannot change it (note *2), so `VinylMode` feedback is the only source for the unit.
- **Fader start**: note 0x66 reports `ChannelFaderStart` `Button(true)` (leaves the bottom), note 0x52 reports `Button(false)` (reaches the bottom). The crossfader start rows use the same bytes (Q08).
- **Beat FX**: `FxUnit::Deck(n)` means "the section as seen on MIDI channel 5 + n"; see `docs/midi-open-questions.md` for the CH SELECT flags and how app-core resolves the target unit.
- **LED values**: lights send 0x7F on and 0x00 off. `Blink` on PLAY is a software timer (500 ms). The channel meter is linear 0-127 over the level app-core publishes. `FxOn` sends 0x7F for on, which the device shows as a blink (note *4, Q10).
