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

## Open questions
- Zoom levels needed (proposal in `CONTEXT/analysis.md`).
- Colour mapping for low/mid/high bands (design-system tokens).
