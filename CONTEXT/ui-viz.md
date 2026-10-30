# ui-viz

Owner: ui-viz-dev.

## Purpose
High-rate visual components: zoomed frequency-coloured waveform with beat grid and cue markers (WebGL2), overview waveform, knobs, jog wheels and pads (Canvas 2D), level meters.

## Owned paths
- `ui/src/viz/**` (plus a Storybook story per component)

## Depends on
- Peaks: raw `ArrayBuffer` from `get_track_peaks` via `ui/src/ipc/raw.ts`, layout `contracts::peaks` v1 (magic `ODPK`, 24-byte header, 4 bytes per point: peak, low, mid, high).
- Telemetry store (non-React) fed by `engine-telemetry`: `position` and `duration` in engine frames, `sample_rate`, `beat_phase`, `active_loop`, meters (linear).
- `BeatGrid` (anchor seconds, bpm, beats per bar) and `Cue` (seconds) DTOs.
- Design tokens for every colour and size.

## Rules
- Render from `requestAnimationFrame` at 60 fps; never re-render React per frame.
- Upload peak data to GPU once per track/zoom level; per frame only update uniforms (playhead, zoom).
- Handle `number | null` floats from bindings (null = unknown).
- Respect `prefers-reduced-motion` for non-essential animation.

## Acceptance per milestone
- M2: overview and zoomed waveform from real peak blobs, 60 fps with 2 decks on a mid-range laptop (measure and report frame times), decoder for the blob with unit tests including bad magic and truncated input.
- M3: beat grid overlay, tempo-scaled scrolling.
- M4: cue and loop markers with colours, pads canvas with mode colours.
- M1/M4: knobs, faders, jog (scratch input forwarding through commands), meters.

## M4 status
- Peaks: `get_track_peaks(track, level)`; levels 0 to 2 are detail (64, 256, 1024 frames per point), 3 is the overview. Empty payload means not analysed yet: the UI retries and listens to `analysis-ready`. Decoder `viz/peaks.ts`, per-track cache `viz/peakCache.ts` (6 tracks).
- Renderers are Canvas 2D (WebGL2 not done). Columns are one device pixel wide on a fixed world grid so data does not shimmer while scrolling; the level is chosen so a column covers at least one point, and deep zoom interpolates between points.
- Playhead: telemetry anchor plus rate, extrapolated per frame, errors under 0.2 s bled off with an 80 ms time constant, snaps on seeks (`viz/interp.ts`).
- Zoom windows 2, 4, 8, 16, 32, 64 s. Wheel, +/- buttons, `+` and `-` keys while the waveform has focus. No global shortcut yet.
- Scrub drags the waveform and sends `seek` once per frame; there is no jog-search scrub.
- Jog: platter top touches (`jog_touch`) and turns (`jog_turn`), outer ring nudges, Shift drag sends `jog_search` at `SEARCH_BEATS_PER_REV` (16, a UI choice, TODO(verify) with the engine owner).
- Components take a frame source or reader prop, so stories run without the store. Storybook for `viz`: `pnpm -C ui storybook:viz` (port 6007); `build-storybook` builds both it and the design system.
- Cue colours come from the `--cue-1..8` tokens by slot; `Cue.color` is ignored so every colour stays a token.
- Perf: not measured in a browser (none available in CI-less headless setup). JS cost of one waveform frame on a 10 minute track at 2400 device px, canvas calls stubbed, p95: 2.0 ms (2 s window), 0.5 ms (8 s), 1.3 ms (64 s). Rasterisation is not included. Run the `Viz/Perf` story for real frame times; it writes `window.__vizPerf`.

## Open questions
- Zoom levels needed (proposal in `CONTEXT/analysis.md`).
- Colour mapping for low/mid/high bands (design-system tokens).
