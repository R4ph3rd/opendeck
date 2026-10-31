# Keyboard shortcuts

Milestone 2 defaults for both decks. There is no source for "standard" DJ-software shortcuts, so these are provisional; these are the defaults. Settings, Keyboard lists every action, rebinds by key capture, flags conflicts (both actions show a badge) and resets to defaults. Overrides are stored as the UI preference `shortcuts` (library settings, via `get_ui_pref` / `set_ui_pref`). The defaults and lookups live in `ui/src/lib/shortcuts.ts`, the store in `ui/src/state/shortcuts.ts`. Modifier combinations, Escape and Tab cannot be bound.

Keys are physical positions (`KeyboardEvent.code`), so they stay put on any keyboard layout.

| Action | Deck A | Deck B |
|---|---|---|
| Play or pause | `Space` | `Enter` |
| CUE (hold to preview, release to return) | `F1` | `F2` |
| SYNC on or off | `S` | `K` |
| QUANTIZE on or off | `Q` | `P` |
| SLIP on or off | `A` | `L` |
| Loop in | `Z` | `N` |
| Loop out | `X` | `M` |
| RELOOP or EXIT | `C` | `,` |
| Beat jump back (selected size) | `D` | `H` |
| Beat jump forward (selected size) | `F` | `J` |
| Hot cue 1 to 8 | `1` to `8` | Numpad `1` to `8` |

Hot cues: an empty slot stores the playhead, a set slot jumps to it (hold to preview when paused), `Shift` + key deletes a set slot.

Mouse only: loop sizes (1/4 to 32 beats) and `Shift` + click or the ROLL toggle for loop roll (rolls while held), beat jump size, MASTER, sync mode, mixer controls. Double-click a knob or fader to reset it.

Rules:

- `Space` and `Enter` are ignored while a text field, select, slider, dialog or any focused button has the key (a focused button activates on both). Clicking PLAY or CUE with the mouse releases focus so `Space` and `F1` work right after.
- Letter, digit and numpad keys are ignored only while a text field, select, listbox or dialog has focus. A focused button or range slider does not block them.
- `F1` and `F2` work from anywhere.
- Key repeat does not re-trigger an action.
- CUE, hot cues and loop roll are released when the window loses focus, so nothing stays held.
- On a focused CUE button, `Space` or `Enter` hold and release it like the mouse.
- The progress bar takes `Left` and `Right` to seek 5 seconds when focused.

Headless runs: set `OPENDECK_OFFLINE_AUDIO=1` to replace the audio device with the offline renderer (no output, same engine and telemetry).
