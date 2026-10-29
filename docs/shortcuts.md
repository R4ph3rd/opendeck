# Keyboard shortcuts

Milestone 1 defaults. There is no source for "standard" DJ-software shortcuts, so these are provisional; the editable shortcut map arrives in M8 (`get_shortcuts` / `set_shortcuts` in `docs/ipc.md`).

| Key | Action |
|---|---|
| `Space` | Play or pause deck A. |
| `F1` | CUE on deck A. Hold to preview from the cue point, release to return. |

Rules:

- Shortcuts are ignored while a text field, select, slider, dialog or any focused button has the keys (a focused button activates on `Space`). Clicking PLAY or CUE with the mouse releases focus so `Space` and `F1` work right after.
- Key repeat does not re-trigger an action.
- CUE is released when the window loses focus, so a deck never stays in preview.
- On a focused CUE button, `Space` or `Enter` hold and release it like the mouse.
- The progress bar takes `Left` and `Right` to seek 5 seconds when focused.

Headless runs: set `OPENDECK_OFFLINE_AUDIO=1` to replace the audio device with the offline renderer (no output, same engine and telemetry).
