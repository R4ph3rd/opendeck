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
