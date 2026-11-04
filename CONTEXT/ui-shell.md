# ui-shell

Owner: ui-shell-dev.

## Purpose
React app shell: layout (2 decks shown, 4-deck ready), deck headers and transport controls, mixer panel, middle panel that follows the active pad mode per deck, collection view and playlists, import progress, settings (audio device, host API, buffer size, MIDI device, layout, shortcuts), MIDI monitor and learn UI.

## Owned paths
- `ui/src/**` except `ui/src/viz/**` and `ui/src/ipc/bindings.ts` (generated)
- `ui/index.html`, `ui/vite.config.ts`, `ui/eslint.config.js`, `ui/tsconfig.json`, `ui/package.json` (coordinate with design-system-dev and ui-viz-dev)

## Depends on
- `ui/src/ipc/bindings.ts` (generated, never hand-written duplicates), `ui/src/ipc/raw.ts` for byte payloads.
- `@opendeck/design-system` components and tokens (wave 0b onward).
- `ui/src/viz` components (waveforms, knobs, jog, pads, meters).

## Libraries (ADR 0001)
- React 19, TypeScript strict, Vite, Zustand, TanStack Table + Virtual, Motion, Vitest + Testing Library.

## Rules
- Design-system components only; no raw hex or magic numbers.
- Telemetry goes to a non-React store (Zustand vanilla) updated from `events.engineTelemetry`; React components subscribe to coarse state only.
- Pad mode lives in app-core; the UI reads it via `get_pad_mode` and `pad-mode-changed`, and changes it via `set_pad_mode`. Never keep a separate source of truth.
- Copy: concise, direct, no em-dashes.

## Acceptance per milestone
- M0: Vite + React shell calling `app_info`; typecheck, lint, test green. **Done in wave 0a.**
- M1: deck and mixer panels wired to commands; settings audio page.
- M2: collection view (virtualized, 50k rows smooth), playlists tree with drag and drop, import progress UI. **Done in M3 (wave 3b)**, with the Files tab, saved sets and key notation toggle; see `docs/STATUS.md` for gaps.
- M4: middle panel per pad mode (8 modes), hot cue colours and names editing. **Done in M7**: `components/pads/`, one view per mode per deck, mode tabs, persisted split (`opendeck.layout.main`).
- M6: MIDI settings, monitor and learn.
- M8: layout presets and shortcut editor.

## Performance layout (reference rework)
- Bands top to bottom: menu row (`TopBar`), zoomed waveforms, decks with the slim mixer (`DeckRow`), middle row (`MiddleRow`: Beat FX 1, pad-mode or sampler panes, Beat FX 2), browser. Sizes come from the LAYOUT SIZES tokens, see `docs/design/theming.md`.
- Each deck holds its own hot cue grid, pad mode tabs, beat loop and jump strip. The middle row shows the sampler for a deck in HOT CUE mode (SAMPLER toggle) and the mode view for any other pad mode.
- Menu row: mode, FX, SAMPLER, REC (disabled, no recording yet), VIEW, IMPORT | wordmark | layout selector, MIDI and audio status, clock, help, settings. No CPU load readout: telemetry has no such field.
- Browser Preview column is a disabled placeholder: auditioning from the browser needs an engine command.

## Open questions
- Keyboard shortcut defaults (no source yet).
- Window layout for 4 decks.
