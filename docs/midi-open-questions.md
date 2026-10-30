# MIDI open questions (DDJ-FLX4)

Sources, both kept locally and git-ignored in `docs/sources/` (copyright of their owner):

- **List**: Pioneer DJ "DDJ-FLX4 List of MIDI messages" E1, Ver 1.0 (5 pages). The only source for bytes.
- **Manual**: Pioneer DJ DDJ-FLX4 operating instructions (French). Used for what a control does, never for bytes.

A mapping value without a source carries `verify` in `mappings/ddj-flx4.json`, and its text starts with the id of a row below. A test checks that every id used in the mapping has a `TODO(verify)` row here, and that the counts below match the file. Nothing marked `TODO(verify)` is specified until it is checked against hardware.

Verify markers in `ddj-flx4.json`: 165, over 9 questions (Q02, Q04, Q07, Q08, Q09, Q10, Q11, Q12, Q13).

| ID | Question | Source checked | Status |
|---|---|---|---|
| Q01 | Shift-layer pad mode pairs: which mode does each shifted pad-mode button select? | Manual p.17-18: HOT CUE to Keyboard, PAD FX 1 to PAD FX 2, BEAT JUMP to Beat Loop, SAMPLER to Key Shift. List 5-1 to 5-4 (+SHIFT notes), 5-5 (note high nibble per mode) | Resolved. Matches `PadMode::shift_partner` |
| Q02 | Jog ticks per revolution (platter, side, shifted search). The list gives the encoding (0x41 up, 0x3F down) but no resolution | List 1-4 | TODO(verify). Mapping uses a placeholder scale of 1/1024 revolutions per tick on the 8 jog rotation controls; measure on hardware |
| Q03 | 14-bit controls: MSB/LSB send order and what the device expects. The list gives the pairs (MSB CC n, LSB CC n+32) and min/max, not the order | List 1-11, 2-5, 3-1, 3-3 to 3-5, 3-7 to 3-12 | TODO(verify). No mapping entry carries it: the decoder emits on every LSB, and on an MSB alone after 5 ms (`DEFAULT_MSB_TIMEOUT_US`), so both orders end on the right value. Worst case is one transient value off by at most 127/16383 when the LSB arrives first |
| Q04 | Level meter curve: how a level maps to the 0x00-0x7F value. The list gives colour bands only (Green1 0x26-0x40, Green2 0x41-0x56, Orange1 0x57-0x64, Orange2 0x65-0x76, Red 0x77-0x7F) | List 3-15 | TODO(verify). Linear 0-127 over the application level; app-core owns the dB curve (ADR 0007) |
| Q05 | Sampler slot sharing between decks: are the 8 pads shared or per deck? The list shows 8 SAMPLER pads per deck (notes 0x30-0x37) on channels 8 and 10 | List 5-5 | TODO(verify). No mapping entry carries it: pads are reported per deck and app-core decides the slots |
| Q06 | 4-channel audio output routing (1/2 master, 3/4 headphones) | Not a MIDI matter, not in the list | TODO(verify). Audio engine and hardware test |
| Q07 | Beat FX LEVEL/DEPTH: the channel column prints 6 but the status column prints B4 (channel 5) | List 2-5 | TODO(verify). Mapping follows the status byte (channel 5); the other 2-x rows are consistent |
| Q08 | Crossfader start rows (3-8) print channel 7 in the channel column but status 0x90 and 0x91 with notes 0x66 and 0x52, byte for byte the channel fader start rows (3-7) | List 3-7, 3-8 | TODO(verify). Mapped as `channel_fader_start` of deck 1 or 2 by the status byte; if the crossfader really sends on channel 7 its start messages are unmapped |
| Q09 | BEAT SYNC only sends when the finger is released (note *3). The value it sends (0x7F then 0x00, or 0x7F only) is not stated | List 1-10, note *3 | TODO(verify). Mapping reports a press edge on a value above 0 |
| Q10 | FX ON/OFF LED (note *4): blinks on NOTE ON, lights on NOTE OFF. The MIDI-OUT columns of 2-6 are empty and no address is given | List 2-6, note *4. Manual p.69 (the button blinks while the effect is on) | TODO(verify). Mapping sends the input note (0x47 on channel 5 for CH1, 6 for CH2): on = 0x7F (device blinks), off = 0x00 (steady lit). Whether a true NOTE OFF (0x8n) is needed is unknown |
| Q11 | LOADED track-load illumination: only the ON value 0x7F is given | List, illumination table | TODO(verify). Off assumed 0x00 |
| Q12 | Which application state lights the IN and OUT LEDs (1-5, 1-6 MIDI-OUT) | List 1-5, 1-6. Manual p.15-16 | TODO(verify). `loop_active` lights both IN and OUT |
| Q13 | +SHIFT LED bank: the list gives MIDI-OUT for the +SHIFT notes (play 0x0E, cue 0x48, IN 0x4C, OUT 0x4E, sync 0x60, CH CUE 0x68) and for pads on channels 9 and 11, with no separate state | List 1-1, 1-2, 1-5, 1-6, 1-10, 3-6, 5-5 | TODO(verify). The +SHIFT bank mirrors the base state. The device is assumed to show that bank while SHIFT is held |
| Q14 | Pad LED colours: the list shows only on/off values for pads | List 5-5 | TODO(verify). Pads are single colour in the mapping; colour or brightness codes, if any, are not in the list |
| Q15 | BEAT SYNC long press (1-10, note 0x5C): sync leader or tempo-only sync? | Manual p.17: pressing at least one second sets the deck as the master | Resolved. Mapped to `sync_leader` |

## Decisions that are not questions

- **Rows left unmapped** are listed in `unmapped` in the mapping, with a reason, and the monitor labels them. They are: PLAY +SHIFT, IN +SHIFT, MASTER CUE +SHIFT (no function in the manual); MIC LEVEL (no microphone input); SMART CFX and SMART FADER with their +SHIFT notes and LEDs (rekordbox features without an OpenDeck action); the Android MONO/STEREO switch; BROWSE +SHIFT rotation (waveform zoom); LOAD +SHIFT (library sort); two inert FX CH SELECT flags that always send 0x00.
- **Duplicate bytes**: TEMPO +SHIFT (1-11) and JOG wheel side +SHIFT (1-4) send the same CC as without SHIFT, so one control serves both.
- **Beat FX addressing**: the hardware has one Beat FX section. `FxUnit::Deck(n)` in the FLX4 mapping means "the section as seen on MIDI channel 5 + n": FX ON/OFF on channel 5 (CH1 and CH1&CH2) is `Deck(0)`, on channel 6 (CH2) is `Deck(1)`; FX SELECT, BEAT, BPM mode and LEVEL/DEPTH only exist on channel 5 and use `Deck(0)`. The CH SELECT slide switch reports two flags (channel 5 note 0x10 for CH1, channel 6 note 0x11 for CH2, 0x7F set, 0x00 clear) mapped to `fx_assign` with `Button(bool)`. app-core turns the flag pair into Ch1, Ch2 or Both and applies section controls to the unit(s) it selects.
- **Soft takeover**: the mapping layer reports raw absolute positions and the FLX4 sends no position dump on connect, so a control has no known value until it moves. Pickup policy belongs to app-core; `MappingEngine::last_absolute` exposes the last position per control.
- **Blink**: the list has no hardware blink code for PLAY, so `playing` blinks by software toggling every 500 ms.
