# dsp-fx

Owner: dsp-fx-dev.

## Purpose
Built-in effects implementing `contracts::fx::Effect`, exposed through an `EffectFactory` with provider id `"builtin"`, plus factory presets.

## Owned paths
- `crates/dsp-fx/**`
- `docs/fx-catalog.md` (create in M5: effect list, parameters with ranges, units, defaults and curves)

## Depends on
- `opendeck-contracts` only (`Effect`, `EffectFactory`, `EffectInfo`, `ParamDescriptor`, `ProcessContext`, `StereoBufferMut`, `FxPreset`). Never the engine (ADR 0002, 0006).

## External crates
- None required. `rustfft` only if a spectral effect needs it. No GPL code (no ports from Mixxx or similar).

## RT rules
- Allocate only in `prepare`. `process`, `set_parameter`, `reset` are allocation-free, lock-free, panic-free.
- Write the fully wet signal; the host does wet/dry and bypass.
- Report latency honestly via `latency_frames`; set `has_tail` for echo/reverb.
- Beat-synced effects derive times from `ProcessContext::{bpm, beat_division, beat_position}` and fall back to a fixed time when `bpm` is `None`.

## Acceptance per milestone
- M5: catalog reviewed; at least echo, reverb, filter (HPF/LPF sweep), flanger, phaser, roll/slicer (beat FX) and a colour filter; each with parameter descriptors from the catalog, an allocation test (`assert_no_alloc` around `process`), a null test (zero input gives zero output after tail) and a sample-rate test (44.1/48/96 kHz).
- M5: factory presets defined in data, recalled through `FxPreset`.
- Later: PAD FX 1/2 banks (pad modes) map to effect presets.

## Open questions
- Which effects PAD FX 1 and PAD FX 2 pads trigger by default (rekordbox defaults are not in the message list).
- Smart CFX knob (FLX4 3-10) behaviour: map to the channel colour filter or ignore.
