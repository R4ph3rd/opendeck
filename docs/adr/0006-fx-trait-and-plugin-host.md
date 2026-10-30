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
- Each unit has `FX_SLOTS_PER_UNIT` (6, raised from 3 in M5, see the addendum) slots processed in order. Sizes are contracts constants because engine arrays use them.

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

## Addendum: engine FX host (M5, 2026-10-30)

`crates/engine/src/fx.rs` (module docs hold the details). Status: accepted.

### Topology and insert points

- `MAX_DECKS` deck units, one master unit, `FX_SEND_BUSES` (2) send units, each a serial chain of `FX_SLOTS_PER_UNIT` (6) slots.
- Deck insert: after trim, EQ and filter, before the PFL tap and the channel fader. The cue hears the effect; the fader also fades a tail.
- Sends: tapped per bus pre-fader (after the insert chain) or post-fader (after channel fader and crossfader side, the default). Returns sum into the master bus before the master unit, so the master chain also processes them.
- A send unit with no slot enabled returns silence, not its input.

### Controller units ("1&2")

A hardware Beat FX unit assigned to one channel is that deck's chain. "1&2" is two mirrored instances, one per deck chain: the app sends every `SetFx*` to both and points both at the same tempo source, so they run in lockstep. One instance on the summed channels is not offered (no summing unit in the contracts; per-channel instances keep each channel's fader, EQ and cue).

### Tempo sources

Per unit: `Auto` (deck units: own deck; master and send units: the sync leader, else the first playing deck with a grid), `Deck(id)` (forced deck), `Tap(bpm)` (private beat counter from 0, play flag from the usual source).

### Timed engage and release FX

- Timed enable/disable at an engine frame (`EngineTelemetry::engine_time` clock), sample accurate inside the block, optional 1 ms ramp for pad FX. The effect is reset on the engage frame. Up to `2 * MAX_DECKS * FX_SLOTS_PER_UNIT` (48) events wait; more act at once; past times act on the next block's first frame.
- Release FX: at one frame, bypass the main slot (10 ms) and engage a release slot (1 ms) for a hold time, then disable it so a tail effect rings out.

### Tail handling

Bypass is a 10 ms crossfade. A `has_tail` effect gets a faded-out input and keeps running until its output stays under -90 dBFS for 4096 frames, or `MAX_TAIL_FRAMES` (480 000) pass.

### Latency compensation

Per slot: the dry path runs through a delay line of the effect's `latency_frames`, read once at install and capped at `MAX_LATENCY_FRAMES` (16 384). It stays in the path while bypassed, so toggling never shifts timing. Limit: compensation is inside the slot only. A deck with a lookahead effect is late against the other decks by that latency; there is no cross-deck alignment.

### Control surface

Effects, timed engage, release FX, tempo source and send tap are `EngineHandle` methods (`install_effect`, `clear_effect`, `schedule_fx_enabled`, `release_fx`, `set_fx_tempo_source`, `set_fx_send_tap`) that app-core calls directly. They are not `EngineCommand` variants: effects are boxes on a dedicated hand-off ring and the rest are engine-internal types. Enable, parameters, mix, beat division and send levels stay `EngineCommand`s. `EngineTelemetry` reports per unit `{enabled, active, level}` in `fx_decks`, `fx_master` and `fx_sends`.

### Dedicated deck slots (M5, 2026-10-30)

`FX_SLOTS_PER_UNIT` went from 3 to 6 (approved contract change). With 3 slots, the third Beat FX effect of a Multi unit shared one slot with Sound Color FX, pad FX and Release FX, so a full Multi unit switched those off on its channels. Each deck chain now has one slot per feature, in processing order:

| Slot | Use |
|---|---|
| 0 to 2 | Beat FX of the unit assigned to the channel (Multi mode; Single uses slot 0) |
| 3 | Sound Color FX |
| 4 | Pad FX |
| 5 | Release FX (SHIFT+ON/OFF and release pads) |

Master and send units have the same 6 slots; the send units in the app model use slots 0 to 2 (`FX_CHAIN_SLOTS` in app-core). Engine arrays, the timed event queue and the FX hand-off and return rings scale with the constant (`FX_HANDOFF_RING = FX_UNITS * FX_SLOTS_PER_UNIT`, `FX_RETURN_RING` twice that). The `utility_blocked` field of `FxStateDto` is gone. CPU (release, 48 kHz, 512 frames, gain effect in every slot): 2 decks x 6 slots + master x 6 slots run at 2.1 % of realtime, worst block 2.3 ms.
