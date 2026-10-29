# contracts

Owner: architect. Every other agent reads it; nobody else edits it.

## Purpose
Cross-crate types and traits: IDs, units, key, beat grid, cues, tempo, mixer, pad modes, deck and mixer snapshots, `EngineCommand`, `EngineTelemetry`, `EngineEvent`, `Effect` / `EffectFactory` / `EffectRegistry`, `Analyzer`, peak blob format, `ControlAction` / `ControlValue` / `FeedbackTarget`, job progress, audio settings DTOs.

## Owned paths
- `crates/contracts/**`

## Depends on
- Nothing internal (ADR 0002). Optional `serde`, `specta` features.

## External crates
- `serde` (feature `serde`), `specta` `=2.0.0-rc.26` (feature `specta`, implies `serde`). Dev: `serde_json`.

## Rules
- Every public item has a doc comment with units, invariants and RT constraints.
- Persisted/IPC positions are `Seconds`; engine positions are `Frames` at the engine rate.
- RT types (`EngineCommand`, `EngineTelemetry`, `EngineEvent`, `FrameRegion`, ids) stay `Copy` with no drop glue; tests assert it. `EngineCommand` must stay <= 40 bytes.
- Bounded ids (`DeckId`, `HotCueIndex`, `SamplerSlotId`, `FxSlotId`) validate on construction and deserialization so the engine can index arrays without panics.
- Adding a `ControlAction` or `FeedbackTarget` variant updates `mappings/schema/mapping.schema.json` in the same commit (the midi drift test enforces the reverse direction).
- Adding an IPC DTO: derive specta behind `cfg_attr(feature = "specta")`, add it to the `contract_dtos_export` test in `src-tauri`.

## Acceptance per milestone
- M0: crate compiles with no features, `serde`, `specta`; all tests pass. **Done in wave 0a.**
- M1: add `AudioDeviceInfo`, `AudioStatus`.
- M2: add `TrackQuery`, `TrackRow`, `TrackDetail`, `TrackPatch`, `PlaylistNode`, `LibraryChange`.
- M3: add `TrackAnalysis`; decide tempo-map type if variable-tempo tracks are in scope.
- M5: add `FxState`.
- M6: add `MidiPorts`, `MappingInfo`, `MidiMonitorEntry`, `MidiLearnResult`, `MidiStatus`.
- M7: add `SavedSetId`, `SavedSet`, `SamplerState`.

## Open questions
- Sampler slot count: 16 assumed (8 pads per deck side, message list 5-5); TODO(verify).
- Pad mode SHIFT pairing (mode n with n + 4) inferred from pad note layout; TODO(verify) against the FLX4 manual.
- Negative playhead positions (beat jump before track start) are clamped to 0 in v1; revisit if needed.
