# FX catalog

Source: not verified against the official manual; names and ranges are common DJ-effect conventions chosen by the project. TODO(verify) when the DDJ-FLX4 operating manual or rekordbox manual is available.

The DDJ-FLX4 operating manual (French edition, DRI1805A) is now available, but it does not list effect types or parameter ranges (it defers to the rekordbox manual). Every per-effect name, parameter and range below therefore stays TODO(verify). The manual does confirm the control model, recorded in the next section. No value here is taken from Pioneer DJ or AlphaTheta documentation.

The parameter tables are generated from `crates/dsp-fx/src/catalog.rs`. A test fails when this file drifts from the data. To refresh after editing the data, run `OPENDECK_BLESS_FX_CATALOG=1 cargo test -p opendeck-dsp-fx doc_tables`. Verified values replace the data in `catalog.rs` with no other code change.

## Verified control model

Source: DDJ-FLX4 operating manual (French, DRI1805A), page numbers are PDF pages. The text is not reproduced here.

- Beat FX (p.31-32, 68-69): BEAT FX SELECT cycles the effect (SHIFT cycles backwards). BEAT FX CH SELECT picks channel 1, channel 2 or 1&2; 1&2 syncs beat, type and the rest between FX unit 1 and unit 2 (not available in Single mode). BEAT left/right decreases/increases the beat count (or the parameter) synced to the effect. LEVEL/DEPTH adjusts the effect parameter. ON/OFF toggles the effect; SHIFT+ON/OFF release triggers the Release FX. Tempo source is BPM Auto (track BPM, default) or BPM Tap (SHIFT+BEAT right taps). The FX panel offers Multi mode and Single mode.
- Sound Color FX (p.26-27, 70): one CFX knob per channel, centre leaves the sound unchanged. Smart CFX presets exist (the manual defers their content to the app documentation).
- Pad FX (p.58-59): Pad FX 1 and Pad FX 2 modes, each with a SHIFT layer, 32 assignable pad effects in total (Effect A to AF). Holding a pad turns the effect on, releasing turns it off; behaviour depends on the effect type and the beat setting assigned to the pad. A pad can hold a Release FX that turns the held effect off and applies the release effect.

How the catalog models this:

- Primary parameter: every effect names one parameter (`primary` in the data) that a single LEVEL/DEPTH control drives. The other parameters are for Single-mode editing. Which parameter the hardware control reaches per effect is TODO(verify).
- Beat division: the host passes `ProcessContext::beat_division`. Stepping through the division list with BEAT left/right is a host and UI concern; the accepted range per effect is in the tables below. Values outside it clamp.
- Release effects: a separate category (`release` in the data). Vinyl Brake is the only one so far. A release effect is engaged by a release trigger, while a normal effect is engaged by ON/OFF or a held pad. The Release FX that a pad or SHIFT+ON/OFF fires, and its pairing with the held effect, is host logic (app-core and engine).
- Pad use: hold engages, release disengages. Effects that capture on engage (Roll, Reverse Roll, Helix, Vinyl Brake) re-arm on `reset()`, which the host must call on every engage so the capture starts on the press. Sample-accurate start and press quantization are decided by the host before it calls `reset()` and begins feeding audio.
- Single vs Multi mode and the 1&2 channel link are host topology (ADR 0006); effects stay unaware of them.

## Conventions

- Effects write the fully wet signal. Wet/dry mix (equal power, smoothed), bypass crossfade, tail spill after bypass until the tail is below -90 dBFS, and dry-path latency compensation belong to the host (ADR 0006). `opendeck_dsp_fx::slot::FxSlot` is a reference host that documents and tests these semantics. Effects that need the dry signal internally (Flanger, Phaser) include their own 50/50 mix in the wet output.
- Parameter ids are the position in the table and are stable. Presets store normalized values.
- Curves: linear, log (`min * (max/min)^n`), stepped (n evenly spaced values). Continuous parameters are smoothed (20 ms time constant, control rate 32 frames); stepped parameters switch immediately.
- Beat sync: delay-line effects use `division * 60 / bpm` seconds exactly; buffers are sized for the largest accepted division at 40 bpm. Slower tempos clamp the time. LFO and gate effects lock phase to `ProcessContext::beat_position` when known. Without a tempo, effects fall back to their "no tempo" parameter (Time or Rate).
- LFO period: 4 x division for Flanger, Phaser and Filter (one bar at a division of 1 beat). Trans gates once per division.
- Latency: 0 except Pitch (half of its 40 ms grain).
- Noise, Bit Crusher and Colour Filter have no beat sync; Pitch and Reverb have none either.

## Effects

<!-- BEGIN GENERATED: effect tables (crates/dsp-fx/src/catalog.rs) -->

### Delay (`builtin:delay`)

Clean stereo delay. Wet output is the delayed signal only; the feedback path is low-passed by Tone.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `feedback`.
- Routing: send/return. Tail: yes. Latency: 0 frames.
- Beat sync: delay time = division. Accepted division 0.03125 to 4 beats, factory default 0.5.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |
| 1 | `feedback` | Feedback | % | 0 | 95 | 35 | linear |
| 2 | `tone` | Tone | Hz | 500 | 16000 | 8000 | log |

Factory presets: Eighth (wet 0.5, division 0.5); Long Tail (wet 0.5, division 1); Slapback (wet 0.4, division 0.125).

### Echo (`builtin:echo`)

Darkening echo. Each repeat passes a 100 Hz high-pass, a Damping low-pass and soft saturation, so repeats decay in level and brightness. Decay is the per-repeat gain.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `decay`.
- Routing: send/return. Tail: yes. Latency: 0 frames.
- Beat sync: echo time = division. Accepted division 0.03125 to 4 beats, factory default 0.75.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |
| 1 | `decay` | Decay | % | 0 | 98 | 55 | linear |
| 2 | `damping` | Damping | Hz | 800 | 16000 | 5000 | log |

Factory presets: Dub Echo (wet 0.5, division 0.75); Short Echo (wet 0.4, division 0.25).

### Ping Pong (`builtin:ping_pong`)

Stereo delay that bounces between left and right. The input is summed to mono, the first repeat lands on the left, the next on the right. Width blends the bounce with a centred mono delay.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `feedback`.
- Routing: send/return. Tail: yes. Latency: 0 frames.
- Beat sync: delay time per bounce = division. Accepted division 0.03125 to 4 beats, factory default 0.5.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |
| 1 | `feedback` | Feedback | % | 0 | 95 | 50 | linear |
| 2 | `width` | Width | % | 0 | 100 | 100 | linear |

Factory presets: Wide Bounce (wet 0.5, division 0.5); Tight (wet 0.4, division 0.25).

### Spiral (`builtin:spiral`)

Delay whose feedback path is pitch-shifted, so every repeat moves further up or down by Pitch. The shifter delay is subtracted from the delay time so repeats stay on the grid.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `feedback`.
- Routing: send/return. Tail: yes. Latency: 0 frames.
- Beat sync: delay time = division. Accepted division 0.03125 to 4 beats, factory default 0.5.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |
| 1 | `feedback` | Feedback | % | 0 | 90 | 55 | linear |
| 2 | `pitch` | Pitch | semitones | -12 | 12 | 7 | linear |

Factory presets: Rising (wet 0.5, division 0.5); Falling (wet 0.5, division 0.5).

### Reverb (`builtin:reverb`)

Stereo reverb: eight damped parallel comb filters and four all-pass filters per channel (public-domain Schroeder/Moorer structure) after an optional pre-delay. Size sets the decay time (about 0.5 s to 10 s).

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `size`.
- Routing: send/return. Tail: yes. Latency: 0 frames.
- Beat sync: none.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `size` | Size | % | 0 | 100 | 50 | linear |
| 1 | `damping` | Damping | % | 0 | 100 | 50 | linear |
| 2 | `predelay` | Pre-delay | ms | 0 | 100 | 0 | linear |
| 3 | `width` | Width | % | 0 | 100 | 100 | linear |

Factory presets: Room (wet 0.3, division 1); Hall (wet 0.35, division 1); Cathedral (wet 0.4, division 1).

### Helix (`builtin:helix`)

Freezes the most recent loop of input and repeats it. Input is recorded continuously while Hold is off. Turning Hold on loops the last division of audio; if less than a division was recorded, it records the rest first (output follows the input meanwhile). Output is silent while Hold is off. A division change refreezes.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `decay`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: frozen loop length = division. Accepted division 0.0625 to 4 beats, factory default 1.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `hold` | Hold | - | 0 | 1 | 1 | stepped (2) |
| 1 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |
| 2 | `fade` | Fade | ms | 0.5 | 50 | 5 | log |
| 3 | `decay` | Decay | % | 0 | 50 | 0 | linear |

Factory presets: Freeze Beat (wet 1, division 1); Stutter (wet 1, division 0.25).

### Flanger (`builtin:flanger`)

Swept short delay mixed with the input (the wet signal contains the 50/50 comb). Right channel LFO is 90 degrees ahead. Without tempo the LFO runs at Rate.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `depth`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: LFO period = 4 x division (one bar at a division of 1 beat). Accepted division 0.125 to 32 beats, factory default 1.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `rate` | Rate (no tempo) | Hz | 0.05 | 5 | 0.5 | log |
| 1 | `depth` | Depth | % | 0 | 100 | 70 | linear |
| 2 | `feedback` | Feedback | % | -90 | 90 | 50 | linear |
| 3 | `delay` | Delay | ms | 0.2 | 8 | 1.5 | log |

Factory presets: Jet (wet 0.5, division 2); Subtle (wet 0.4, division 1).

### Phaser (`builtin:phaser`)

Swept all-pass chain mixed with the input (the wet signal contains the 50/50 mix). Depth sweeps up to 1.5 octaves either side of Center. Right channel LFO is 90 degrees ahead.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `depth`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: LFO period = 4 x division (one bar at a division of 1 beat). Accepted division 0.125 to 32 beats, factory default 1.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `rate` | Rate (no tempo) | Hz | 0.05 | 5 | 0.5 | log |
| 1 | `depth` | Depth | % | 0 | 100 | 70 | linear |
| 2 | `feedback` | Feedback | % | 0 | 90 | 40 | linear |
| 3 | `stages` | Stages | - | 2 | 12 | 4 | stepped (6) |
| 4 | `center` | Center | Hz | 100 | 5000 | 800 | log |

Factory presets: Slow Sweep (wet 0.5, division 4); Deep (wet 0.5, division 1).

### Filter (`builtin:filter`)

Resonant low-pass or high-pass whose cutoff sweeps between Low and High on a sine LFO. Depth scales the sweep around the geometric middle. Without tempo the LFO runs at Rate.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `depth`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: LFO period = 4 x division (one bar at a division of 1 beat). Accepted division 0.125 to 32 beats, factory default 1.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `mode` | Mode | - | 0 | 1 | 0 | stepped (2) |
| 1 | `low` | Low | Hz | 40 | 2000 | 200 | log |
| 2 | `high` | High | Hz | 1000 | 18000 | 8000 | log |
| 3 | `resonance` | Resonance | % | 0 | 95 | 40 | linear |
| 4 | `depth` | Depth | % | 0 | 100 | 100 | linear |
| 5 | `rate` | Rate (no tempo) | Hz | 0.05 | 5 | 0.5 | log |

Factory presets: LP Sweep (wet 1, division 2); HP Sweep (wet 1, division 2).

### Trans (`builtin:trans`)

Rhythmic gate. The gate is open for Duty percent of each division and closed (attenuated by Depth) for the rest. Phase locks to the beat grid when the beat position is known; without tempo the gate runs at Rate.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `depth`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: gate period = division. Accepted division 0.03125 to 2 beats, factory default 0.25.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `depth` | Depth | % | 0 | 100 | 100 | linear |
| 1 | `duty` | Duty | % | 10 | 90 | 50 | linear |
| 2 | `smooth` | Smooth | ms | 0.5 | 50 | 5 | log |
| 3 | `rate` | Rate (no tempo) | Hz | 0.5 | 16 | 4 | log |

Factory presets: Sixteenths (wet 1, division 0.25); Eighths Soft (wet 1, division 0.5).

### Roll (`builtin:roll`)

Loop roll of the input. On engage (reset) or a division change it records one division while passing the input, then repeats that slice. Decay lowers the level on every repeat; Fade softens the loop seam.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `decay`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: loop length = division. Accepted division 0.03125 to 4 beats, factory default 0.25.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `decay` | Decay | % | 0 | 50 | 0 | linear |
| 1 | `fade` | Fade | ms | 0.5 | 50 | 2 | log |
| 2 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |

Factory presets: Quarter Roll (wet 1, division 0.25); Fading Roll (wet 1, division 0.5).

### Reverse Roll (`builtin:reverse_roll`)

Like Roll, but the recorded slice repeats backwards.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `decay`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: loop length = division. Accepted division 0.03125 to 4 beats, factory default 0.5.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `decay` | Decay | % | 0 | 50 | 0 | linear |
| 1 | `fade` | Fade | ms | 0.5 | 50 | 2 | log |
| 2 | `time` | Time (no tempo) | ms | 10 | 4000 | 500 | log |

Factory presets: Half Reverse (wet 1, division 0.5); Short Reverse (wet 1, division 0.125).

### Pitch (`builtin:pitch`)

Granular pitch shift with two overlapping grains (40 ms) and a raised-cosine window. Reports its latency for host compensation.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `shift`.
- Routing: insert. Tail: no. Latency: 960 (half of the 40 ms grain) frames.
- Beat sync: none.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `shift` | Shift | semitones | -12 | 12 | 0 | linear |

Factory presets: Up 5 (wet 1, division 1); Down 5 (wet 1, division 1); Octave Up (wet 1, division 1); Octave Down (wet 1, division 1).

### Vinyl Brake (`builtin:vinyl_brake`)

Playback slows to a stop over one division (starts on engage/reset, restarts on a division change). Curve shapes the slow-down from linear (0) to steep (100). Silent once stopped.

- Category: release effect. Primary parameter (LEVEL/DEPTH target): `curve`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: time to a full stop = division. Accepted division 0.25 to 4 beats, factory default 1.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `curve` | Curve | % | 0 | 100 | 40 | linear |
| 1 | `time` | Time (no tempo) | ms | 100 | 4000 | 1000 | log |

Factory presets: Quick Stop (wet 1, division 0.5); Slow Stop (wet 1, division 2).

### Noise (`builtin:noise`)

Adds white noise through a resonant filter (Mode: 0 low-pass, 1 band-pass, 2 high-pass) whose cutoff sweeps on a free-running LFO. Wet output is the input plus the noise.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `level`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: none.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `level` | Level | % | 0 | 100 | 30 | linear |
| 1 | `cutoff` | Cutoff | Hz | 100 | 12000 | 2000 | log |
| 2 | `resonance` | Resonance | % | 0 | 90 | 50 | linear |
| 3 | `sweep` | Sweep | % | 0 | 100 | 100 | linear |
| 4 | `rate` | Rate | Hz | 0.05 | 2 | 0.25 | log |
| 5 | `mode` | Mode | - | 0 | 2 | 1 | stepped (3) |

Factory presets: Riser (wet 1, division 1); Wind (wet 1, division 1).

### Bit Crusher (`builtin:bit_crusher`)

Reduces amplitude resolution (Bits, fractional values allowed) and sample rate (sample-and-hold at Rate; no anti-alias filter, by design).

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `bits`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: none.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `bits` | Bits | - | 1 | 16 | 8 | linear |
| 1 | `rate` | Rate | Hz | 500 | 48000 | 16000 | log |

Factory presets: Lo-Fi (wet 1, division 1); Crunch (wet 1, division 1).

### Colour Filter (`builtin:colour_filter`)

Single-knob 24 dB/octave filter for per-channel use. Colour below 0 is a low-pass sweeping from 20 kHz down to 100 Hz, above 0 a high-pass sweeping from 20 Hz up to 8 kHz, 0 is flat. Resonance raises the second-stage Q.

- Category: normal effect. Primary parameter (LEVEL/DEPTH target): `colour`.
- Routing: insert. Tail: no. Latency: 0 frames.
- Beat sync: none.

| Id | Key | Name | Unit | Min | Max | Default | Curve |
|---:|---|---|---|---:|---:|---:|---|
| 0 | `colour` | Colour | % | -100 | 100 | 0 | linear |
| 1 | `resonance` | Resonance | % | 0 | 100 | 10 | linear |

Factory presets: Low-pass Sweep (wet 1, division 1); High-pass Sweep (wet 1, division 1).

<!-- END GENERATED -->

## Open items

- TODO(verify): effect list, names, parameter ranges, defaults and the beat division list against the rekordbox manual.
- TODO(verify): which effects Pad FX 1 and Pad FX 2 assign by default, and the Release FX set.
- TODO(verify): Smart CFX preset content.
