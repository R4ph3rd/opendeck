# ADR 0006: Effect trait and plugin host boundary

- Status: accepted
- Date: 2026-10-29
- Deciders: architect

## Context

The MVP ships built-in effects (per-deck and master slots, chains, insert and send/return, beat-synced, wet/dry, bypass, presets). VST3 and CLAP hosting comes later and must not require engine changes.

## Decision

### Contract (`crates/contracts/src/fx.rs`)

- `Effect`: one instance. `prepare(sample_rate, max_block_frames)` runs off the audio thread and may allocate. `set_parameter`, `process`, `reset`, `latency_frames` and `info` run on the audio thread and must not allocate, free, lock, block, do I/O, log or panic.
- `process(&mut self, io: StereoBufferMut, &ProcessContext)` is in place, planar stereo `f32`, and writes the **fully wet** signal.
- Parameters are described by `ParamDescriptor` (id, name, unit, min, max, default, curve: linear, logarithmic or stepped). Values crossing the engine boundary are normalized `[0, 1]`, like VST3; descriptors convert to plain values (CLAP). `ParamId` is `u32` and stable, because presets store it.
- `ProcessContext` gives tempo, beat position at block start, selected beat division and play state. Beat-synced effects (`EffectInfo::beat_synced`) derive times from it.
- `EffectInfo` declares routing intent (insert or send/return), whether the effect has a tail and its parameters.
- `EffectFactory` (provider id, list, create) and `EffectRegistry` (in app-core) are the plugin boundary. Built-ins are the `"builtin"` provider in `dsp-fx`. A future `crates/plugin-host` provides `"clap"` and `"vst3"` factories and depends on `contracts` only.

### Host responsibilities (engine slot host)

The engine wraps each instance in a slot that owns everything an external plugin would not:

- wet/dry mix (`SetFxMix`), equal-power, smoothed;
- bypass (`SetFxEnabled`) as a short crossfade; effects with a tail keep processing silent input until the tail decays below -90 dBFS;
- latency compensation for the dry path from `latency_frames`;
- beat division (`SetFxBeatDivision`) passed through `ProcessContext`.

### Topology

- `FxUnit::Deck(deck)`: insert chain after EQ and filter, before the channel fader.
- `FxUnit::Master`: insert chain before master level.
- `FxUnit::Send(bus)`: send/return bus; channels feed it with `SetFxSend`; its return sums into master.
- Each unit has `FX_SLOTS_PER_UNIT` (3) slots processed in order. Sizes are contracts constants because engine arrays use them.

### Lifecycle

`EffectRegistry::create` (control thread) -> `prepare` (loader thread) -> hand-off ring -> audio thread -> on replace or clear, return ring -> dropped on the loader thread (ADR 0003). Changing the sample rate or block size re-prepares off the audio thread and swaps instances.

### Presets

`FxPreset` (effect key, name, normalized params by id, wet, beat division) stored in `fx_presets`. Unknown param ids are ignored on recall so effects can add parameters.

### Parameters come from a catalog

Built-in effects and their parameters are specified in `docs/fx-catalog.md` (dsp-fx-dev, milestone 5), not invented in code.

## Consequences

- External plugins and built-ins look identical to the engine.
- Plugin crashes are not isolated in-process; out-of-process hosting can be added behind the same `EffectFactory` later.
- Effects cannot implement their own dry path; an effect that needs dry signal internally (e.g. freeze) receives it in its input and outputs its wet signal.
