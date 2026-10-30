//! The effect catalog as data.
//!
//! Every parameter id, name, unit, range, default and curve, every beat-sync
//! range and every factory preset lives in [`CATALOG`]. The effects read their
//! indices and ranges from it, the factory builds `EffectInfo` from it and
//! `docs/fx-catalog.md` is generated from it (a test fails when the document
//! drifts). Replacing a value here is the only change needed when verified
//! numbers replace the project's choices.
//!
//! Source: not verified against any official manual. The values are common
//! DJ-effect conventions chosen by the project. `TODO(verify)` when the
//! DDJ-FLX4 operating manual or the rekordbox manual is available.

use opendeck_contracts::fx::{
    EffectInfo, EffectKey, FxRouting, ParamCurve, ParamDescriptor, ParamId, ParamUnit,
};

/// Lowest tempo delay lines are sized for. Slower tempos clamp the delay time.
pub const MIN_BPM: f64 = 40.0;

/// Mapping from normalized to plain value, as data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// Linear.
    Linear,
    /// Logarithmic (`min > 0`).
    Log,
    /// `n` evenly spaced values.
    Stepped(u16),
}

/// One parameter. The id is the position in [`EffectSpec::params`].
#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    /// Stable key used by presets in this crate and by the effect code.
    pub key: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Display unit.
    pub unit: ParamUnit,
    /// Lowest plain value.
    pub min: f32,
    /// Highest plain value.
    pub max: f32,
    /// Default plain value.
    pub default: f32,
    /// Curve.
    pub curve: Curve,
}

/// Beat-sync behaviour of an effect.
#[derive(Debug, Clone, Copy)]
pub struct SyncSpec {
    /// What one beat division controls.
    pub meaning: &'static str,
    /// Smallest accepted division in beats (smaller values clamp).
    pub min_beats: f64,
    /// Largest accepted division in beats (larger values clamp). Sizes the buffers.
    pub max_beats: f64,
    /// Division a fresh slot starts with.
    pub default_beats: f64,
}

/// A factory preset: plain values by parameter key, unlisted keys keep the default.
#[derive(Debug, Clone, Copy)]
pub struct PresetSpec {
    /// Preset name, unique per effect.
    pub name: &'static str,
    /// Plain values by parameter key.
    pub params: &'static [(&'static str, f32)],
    /// Wet/dry mix `[0, 1]`.
    pub wet: f32,
    /// Beat division in beats.
    pub division: f64,
}

/// One effect.
#[derive(Debug, Clone, Copy)]
pub struct EffectSpec {
    /// Stable id, the `EffectKey::id` of the built-in provider.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Key of the primary parameter: the one a single LEVEL/DEPTH control drives.
    /// TODO(verify): which parameter the hardware control maps to per effect.
    pub primary: &'static str,
    /// Release effect: engaged by a release trigger, stops the held effect (not a normal effect).
    pub release: bool,
    /// Intended routing.
    pub routing: FxRouting,
    /// Output continues after input stops.
    pub tail: bool,
    /// Beat-sync behaviour, `None` when not beat-synced.
    pub sync: Option<SyncSpec>,
    /// Latency in frames at 48 kHz, as text (the real value scales with the rate).
    pub latency: &'static str,
    /// One-paragraph behaviour description.
    pub summary: &'static str,
    /// Parameters in display order.
    pub params: &'static [ParamSpec],
    /// Factory presets.
    pub presets: &'static [PresetSpec],
}

impl ParamSpec {
    /// Contract descriptor for this parameter at position `index`.
    pub fn descriptor(&self, index: usize) -> ParamDescriptor {
        ParamDescriptor {
            id: ParamId(index as u32),
            name: self.name.to_owned(),
            unit: self.unit,
            min: self.min,
            max: self.max,
            default: self.default,
            curve: match self.curve {
                Curve::Linear => ParamCurve::Linear,
                Curve::Log => ParamCurve::Logarithmic,
                Curve::Stepped(steps) => ParamCurve::Stepped { steps },
            },
        }
    }
}

impl EffectSpec {
    /// Contract description of this effect.
    pub fn info(&self) -> EffectInfo {
        EffectInfo {
            key: EffectKey {
                provider: crate::PROVIDER_ID.to_owned(),
                id: self.id.to_owned(),
            },
            name: self.name.to_owned(),
            routing: self.routing,
            beat_synced: self.sync.is_some(),
            has_tail: self.tail,
            params: self
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| p.descriptor(i))
                .collect(),
        }
    }

    /// Position of the parameter with `key`.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.params.iter().position(|p| p.key == key)
    }
}

/// Looks an effect up by id.
pub fn find(id: &str) -> Option<&'static EffectSpec> {
    CATALOG.iter().find(|e| e.id == id)
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Compile-time parameter index. Fails the build when the catalog has no such
/// effect or parameter, so effect code cannot drift from the data.
pub const fn param_index(effect: &str, key: &str) -> usize {
    let mut e = 0;
    while e < CATALOG.len() {
        if str_eq(CATALOG[e].id, effect) {
            let mut p = 0;
            while p < CATALOG[e].params.len() {
                if str_eq(CATALOG[e].params[p].key, key) {
                    return p;
                }
                p += 1;
            }
        }
        e += 1;
    }
    panic!("parameter missing from the catalog")
}

const fn p(
    key: &'static str,
    name: &'static str,
    unit: ParamUnit,
    min: f32,
    max: f32,
    default: f32,
    curve: Curve,
) -> ParamSpec {
    ParamSpec {
        key,
        name,
        unit,
        min,
        max,
        default,
        curve,
    }
}

const fn sync(meaning: &'static str, min: f64, max: f64, default: f64) -> Option<SyncSpec> {
    Some(SyncSpec {
        meaning,
        min_beats: min,
        max_beats: max,
        default_beats: default,
    })
}

const fn pre(
    name: &'static str,
    wet: f32,
    division: f64,
    params: &'static [(&'static str, f32)],
) -> PresetSpec {
    PresetSpec {
        name,
        params,
        wet,
        division,
    }
}

use Curve::{Linear, Log, Stepped};
use ParamUnit::{Hertz, Milliseconds, None as Num, Percent, Semitones};

const LFO_PERIOD: &str = "LFO period = 4 x division (one bar at a division of 1 beat)";

const P_TIME: ParamSpec = p(
    "time",
    "Time (no tempo)",
    Milliseconds,
    10.0,
    4000.0,
    500.0,
    Log,
);
const P_RATE: ParamSpec = p("rate", "Rate (no tempo)", Hertz, 0.05, 5.0, 0.5, Log);

/// Every built-in effect.
pub const CATALOG: &[EffectSpec] = &[
    EffectSpec {
        id: "delay",
        primary: "feedback",
        release: false,
        name: "Delay",
        routing: FxRouting::SendReturn,
        tail: true,
        sync: sync("delay time = division", 1.0 / 32.0, 4.0, 0.5),
        latency: "0",
        summary: "Clean stereo delay. Wet output is the delayed signal only; the feedback path is low-passed by Tone.",
        params: &[
            P_TIME,
            p("feedback", "Feedback", Percent, 0.0, 95.0, 35.0, Linear),
            p("tone", "Tone", Hertz, 500.0, 16000.0, 8000.0, Log),
        ],
        presets: &[
            pre("Eighth", 0.5, 0.5, &[("feedback", 35.0)]),
            pre(
                "Long Tail",
                0.5,
                1.0,
                &[("feedback", 70.0), ("tone", 4000.0)],
            ),
            pre("Slapback", 0.4, 0.125, &[("feedback", 10.0)]),
        ],
    },
    EffectSpec {
        id: "echo",
        primary: "decay",
        release: false,
        name: "Echo",
        routing: FxRouting::SendReturn,
        tail: true,
        sync: sync("echo time = division", 1.0 / 32.0, 4.0, 0.75),
        latency: "0",
        summary: "Darkening echo. Each repeat passes a 100 Hz high-pass, a Damping low-pass and soft saturation, so repeats decay in level and brightness. Decay is the per-repeat gain.",
        params: &[
            P_TIME,
            p("decay", "Decay", Percent, 0.0, 98.0, 55.0, Linear),
            p("damping", "Damping", Hertz, 800.0, 16000.0, 5000.0, Log),
        ],
        presets: &[
            pre(
                "Dub Echo",
                0.5,
                0.75,
                &[("decay", 70.0), ("damping", 2500.0)],
            ),
            pre("Short Echo", 0.4, 0.25, &[("decay", 40.0)]),
        ],
    },
    EffectSpec {
        id: "ping_pong",
        primary: "feedback",
        release: false,
        name: "Ping Pong",
        routing: FxRouting::SendReturn,
        tail: true,
        sync: sync("delay time per bounce = division", 1.0 / 32.0, 4.0, 0.5),
        latency: "0",
        summary: "Stereo delay that bounces between left and right. The input is summed to mono, the first repeat lands on the left, the next on the right. Width blends the bounce with a centred mono delay.",
        params: &[
            P_TIME,
            p("feedback", "Feedback", Percent, 0.0, 95.0, 50.0, Linear),
            p("width", "Width", Percent, 0.0, 100.0, 100.0, Linear),
        ],
        presets: &[
            pre("Wide Bounce", 0.5, 0.5, &[("feedback", 55.0)]),
            pre("Tight", 0.4, 0.25, &[("feedback", 35.0), ("width", 60.0)]),
        ],
    },
    EffectSpec {
        id: "spiral",
        primary: "feedback",
        release: false,
        name: "Spiral",
        routing: FxRouting::SendReturn,
        tail: true,
        sync: sync("delay time = division", 1.0 / 32.0, 4.0, 0.5),
        latency: "0",
        summary: "Delay whose feedback path is pitch-shifted, so every repeat moves further up or down by Pitch. The shifter delay is subtracted from the delay time so repeats stay on the grid.",
        params: &[
            P_TIME,
            p("feedback", "Feedback", Percent, 0.0, 90.0, 55.0, Linear),
            p("pitch", "Pitch", Semitones, -12.0, 12.0, 7.0, Linear),
        ],
        presets: &[
            pre("Rising", 0.5, 0.5, &[("pitch", 7.0)]),
            pre("Falling", 0.5, 0.5, &[("pitch", -5.0)]),
        ],
    },
    EffectSpec {
        id: "reverb",
        primary: "size",
        release: false,
        name: "Reverb",
        routing: FxRouting::SendReturn,
        tail: true,
        sync: None,
        latency: "0",
        summary: "Stereo reverb: eight damped parallel comb filters and four all-pass filters per channel (public-domain Schroeder/Moorer structure) after an optional pre-delay. Size sets the decay time (about 0.5 s to 10 s).",
        params: &[
            p("size", "Size", Percent, 0.0, 100.0, 50.0, Linear),
            p("damping", "Damping", Percent, 0.0, 100.0, 50.0, Linear),
            p(
                "predelay",
                "Pre-delay",
                Milliseconds,
                0.0,
                100.0,
                0.0,
                Linear,
            ),
            p("width", "Width", Percent, 0.0, 100.0, 100.0, Linear),
        ],
        presets: &[
            pre("Room", 0.3, 1.0, &[("size", 30.0), ("damping", 60.0)]),
            pre("Hall", 0.35, 1.0, &[("size", 70.0), ("damping", 40.0)]),
            pre(
                "Cathedral",
                0.4,
                1.0,
                &[("size", 95.0), ("damping", 30.0), ("predelay", 40.0)],
            ),
        ],
    },
    EffectSpec {
        id: "helix",
        primary: "decay",
        release: false,
        name: "Helix",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync("frozen loop length = division", 1.0 / 16.0, 4.0, 1.0),
        latency: "0",
        summary: "Freezes the most recent loop of input and repeats it. Input is recorded continuously while Hold is off. Turning Hold on loops the last division of audio; if less than a division was recorded, it records the rest first (output follows the input meanwhile). Output is silent while Hold is off. A division change refreezes.",
        params: &[
            p("hold", "Hold", Num, 0.0, 1.0, 1.0, Stepped(2)),
            P_TIME,
            p("fade", "Fade", Milliseconds, 0.5, 50.0, 5.0, Log),
            p("decay", "Decay", Percent, 0.0, 50.0, 0.0, Linear),
        ],
        presets: &[
            pre("Freeze Beat", 1.0, 1.0, &[]),
            pre("Stutter", 1.0, 0.25, &[("decay", 10.0)]),
        ],
    },
    EffectSpec {
        id: "flanger",
        primary: "depth",
        release: false,
        name: "Flanger",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync(LFO_PERIOD, 0.125, 32.0, 1.0),
        latency: "0",
        summary: "Swept short delay mixed with the input (the wet signal contains the 50/50 comb). Right channel LFO is 90 degrees ahead. Without tempo the LFO runs at Rate.",
        params: &[
            P_RATE,
            p("depth", "Depth", Percent, 0.0, 100.0, 70.0, Linear),
            p("feedback", "Feedback", Percent, -90.0, 90.0, 50.0, Linear),
            p("delay", "Delay", Milliseconds, 0.2, 8.0, 1.5, Log),
        ],
        presets: &[
            pre("Jet", 0.5, 2.0, &[("depth", 90.0), ("feedback", 70.0)]),
            pre("Subtle", 0.4, 1.0, &[("depth", 40.0), ("feedback", 20.0)]),
        ],
    },
    EffectSpec {
        id: "phaser",
        primary: "depth",
        release: false,
        name: "Phaser",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync(LFO_PERIOD, 0.125, 32.0, 1.0),
        latency: "0",
        summary: "Swept all-pass chain mixed with the input (the wet signal contains the 50/50 mix). Depth sweeps up to 1.5 octaves either side of Center. Right channel LFO is 90 degrees ahead.",
        params: &[
            P_RATE,
            p("depth", "Depth", Percent, 0.0, 100.0, 70.0, Linear),
            p("feedback", "Feedback", Percent, 0.0, 90.0, 40.0, Linear),
            p("stages", "Stages", Num, 2.0, 12.0, 4.0, Stepped(6)),
            p("center", "Center", Hertz, 100.0, 5000.0, 800.0, Log),
        ],
        presets: &[
            pre("Slow Sweep", 0.5, 4.0, &[("depth", 80.0)]),
            pre("Deep", 0.5, 1.0, &[("stages", 8.0), ("feedback", 70.0)]),
        ],
    },
    EffectSpec {
        id: "filter",
        primary: "depth",
        release: false,
        name: "Filter",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync(LFO_PERIOD, 0.125, 32.0, 1.0),
        latency: "0",
        summary: "Resonant low-pass or high-pass whose cutoff sweeps between Low and High on a sine LFO. Depth scales the sweep around the geometric middle. Without tempo the LFO runs at Rate.",
        params: &[
            p("mode", "Mode", Num, 0.0, 1.0, 0.0, Stepped(2)),
            p("low", "Low", Hertz, 40.0, 2000.0, 200.0, Log),
            p("high", "High", Hertz, 1000.0, 18000.0, 8000.0, Log),
            p("resonance", "Resonance", Percent, 0.0, 95.0, 40.0, Linear),
            p("depth", "Depth", Percent, 0.0, 100.0, 100.0, Linear),
            P_RATE,
        ],
        presets: &[
            pre("LP Sweep", 1.0, 2.0, &[]),
            pre("HP Sweep", 1.0, 2.0, &[("mode", 1.0), ("resonance", 55.0)]),
        ],
    },
    EffectSpec {
        id: "trans",
        primary: "depth",
        release: false,
        name: "Trans",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync("gate period = division", 1.0 / 32.0, 2.0, 0.25),
        latency: "0",
        summary: "Rhythmic gate. The gate is open for Duty percent of each division and closed (attenuated by Depth) for the rest. Phase locks to the beat grid when the beat position is known; without tempo the gate runs at Rate.",
        params: &[
            p("depth", "Depth", Percent, 0.0, 100.0, 100.0, Linear),
            p("duty", "Duty", Percent, 10.0, 90.0, 50.0, Linear),
            p("smooth", "Smooth", Milliseconds, 0.5, 50.0, 5.0, Log),
            p("rate", "Rate (no tempo)", Hertz, 0.5, 16.0, 4.0, Log),
        ],
        presets: &[
            pre("Sixteenths", 1.0, 0.25, &[]),
            pre(
                "Eighths Soft",
                1.0,
                0.5,
                &[("depth", 80.0), ("smooth", 15.0)],
            ),
        ],
    },
    EffectSpec {
        id: "roll",
        primary: "decay",
        release: false,
        name: "Roll",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync("loop length = division", 1.0 / 32.0, 4.0, 0.25),
        latency: "0",
        summary: "Loop roll of the input. On engage (reset) or a division change it records one division while passing the input, then repeats that slice. Decay lowers the level on every repeat; Fade softens the loop seam.",
        params: &[
            p("decay", "Decay", Percent, 0.0, 50.0, 0.0, Linear),
            p("fade", "Fade", Milliseconds, 0.5, 50.0, 2.0, Log),
            P_TIME,
        ],
        presets: &[
            pre("Quarter Roll", 1.0, 0.25, &[]),
            pre("Fading Roll", 1.0, 0.5, &[("decay", 25.0)]),
        ],
    },
    EffectSpec {
        id: "reverse_roll",
        primary: "decay",
        release: false,
        name: "Reverse Roll",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync("loop length = division", 1.0 / 32.0, 4.0, 0.5),
        latency: "0",
        summary: "Like Roll, but the recorded slice repeats backwards.",
        params: &[
            p("decay", "Decay", Percent, 0.0, 50.0, 0.0, Linear),
            p("fade", "Fade", Milliseconds, 0.5, 50.0, 2.0, Log),
            P_TIME,
        ],
        presets: &[
            pre("Half Reverse", 1.0, 0.5, &[]),
            pre("Short Reverse", 1.0, 0.125, &[("decay", 20.0)]),
        ],
    },
    EffectSpec {
        id: "pitch",
        primary: "shift",
        release: false,
        name: "Pitch",
        routing: FxRouting::Insert,
        tail: false,
        sync: None,
        latency: "960 (half of the 40 ms grain)",
        summary: "Granular pitch shift with two overlapping grains (40 ms) and a raised-cosine window. Reports its latency for host compensation.",
        params: &[p("shift", "Shift", Semitones, -12.0, 12.0, 0.0, Linear)],
        presets: &[
            pre("Up 5", 1.0, 1.0, &[("shift", 5.0)]),
            pre("Down 5", 1.0, 1.0, &[("shift", -5.0)]),
            pre("Octave Up", 1.0, 1.0, &[("shift", 12.0)]),
            pre("Octave Down", 1.0, 1.0, &[("shift", -12.0)]),
        ],
    },
    EffectSpec {
        id: "vinyl_brake",
        primary: "curve",
        release: true,
        name: "Vinyl Brake",
        routing: FxRouting::Insert,
        tail: false,
        sync: sync("time to a full stop = division", 0.25, 4.0, 1.0),
        latency: "0",
        summary: "Playback slows to a stop over one division (starts on engage/reset, restarts on a division change). Curve shapes the slow-down from linear (0) to steep (100). Silent once stopped.",
        params: &[
            p("curve", "Curve", Percent, 0.0, 100.0, 40.0, Linear),
            p(
                "time",
                "Time (no tempo)",
                Milliseconds,
                100.0,
                4000.0,
                1000.0,
                Log,
            ),
        ],
        presets: &[
            pre("Quick Stop", 1.0, 0.5, &[("curve", 20.0)]),
            pre("Slow Stop", 1.0, 2.0, &[("curve", 60.0)]),
        ],
    },
    EffectSpec {
        id: "noise",
        primary: "level",
        release: false,
        name: "Noise",
        routing: FxRouting::Insert,
        tail: false,
        sync: None,
        latency: "0",
        summary: "Adds white noise through a resonant filter (Mode: 0 low-pass, 1 band-pass, 2 high-pass) whose cutoff sweeps on a free-running LFO. Wet output is the input plus the noise.",
        params: &[
            p("level", "Level", Percent, 0.0, 100.0, 30.0, Linear),
            p("cutoff", "Cutoff", Hertz, 100.0, 12000.0, 2000.0, Log),
            p("resonance", "Resonance", Percent, 0.0, 90.0, 50.0, Linear),
            p("sweep", "Sweep", Percent, 0.0, 100.0, 100.0, Linear),
            p("rate", "Rate", Hertz, 0.05, 2.0, 0.25, Log),
            p("mode", "Mode", Num, 0.0, 2.0, 1.0, Stepped(3)),
        ],
        presets: &[
            pre("Riser", 1.0, 1.0, &[("mode", 2.0), ("rate", 0.1)]),
            pre("Wind", 1.0, 1.0, &[("level", 20.0), ("resonance", 70.0)]),
        ],
    },
    EffectSpec {
        id: "bit_crusher",
        primary: "bits",
        release: false,
        name: "Bit Crusher",
        routing: FxRouting::Insert,
        tail: false,
        sync: None,
        latency: "0",
        summary: "Reduces amplitude resolution (Bits, fractional values allowed) and sample rate (sample-and-hold at Rate; no anti-alias filter, by design).",
        params: &[
            p("bits", "Bits", Num, 1.0, 16.0, 8.0, Linear),
            p("rate", "Rate", Hertz, 500.0, 48000.0, 16000.0, Log),
        ],
        presets: &[
            pre("Lo-Fi", 1.0, 1.0, &[("bits", 8.0), ("rate", 12000.0)]),
            pre("Crunch", 1.0, 1.0, &[("bits", 4.0), ("rate", 6000.0)]),
        ],
    },
    EffectSpec {
        id: "colour_filter",
        primary: "colour",
        release: false,
        name: "Colour Filter",
        routing: FxRouting::Insert,
        tail: false,
        sync: None,
        latency: "0",
        summary: "Single-knob 24 dB/octave filter for per-channel use. Colour below 0 is a low-pass sweeping from 20 kHz down to 100 Hz, above 0 a high-pass sweeping from 20 Hz up to 8 kHz, 0 is flat. Resonance raises the second-stage Q.",
        params: &[
            p("colour", "Colour", Percent, -100.0, 100.0, 0.0, Linear),
            p("resonance", "Resonance", Percent, 0.0, 100.0, 10.0, Linear),
        ],
        presets: &[
            pre("Low-pass Sweep", 1.0, 1.0, &[("colour", -60.0)]),
            pre("High-pass Sweep", 1.0, 1.0, &[("colour", 60.0)]),
        ],
    },
];

// ---------------------------------------------------------------------------
// Document generator
// ---------------------------------------------------------------------------

/// First line of the generated block in `docs/fx-catalog.md`.
pub const DOC_BEGIN: &str =
    "<!-- BEGIN GENERATED: effect tables (crates/dsp-fx/src/catalog.rs) -->";
/// Last line of the generated block in `docs/fx-catalog.md`.
pub const DOC_END: &str = "<!-- END GENERATED -->";

fn unit_label(u: ParamUnit) -> &'static str {
    match u {
        ParamUnit::None => "-",
        ParamUnit::Percent => "%",
        ParamUnit::Decibels => "dB",
        ParamUnit::Hertz => "Hz",
        ParamUnit::Milliseconds => "ms",
        ParamUnit::Beats => "beats",
        ParamUnit::Semitones => "semitones",
    }
}

fn curve_label(c: Curve) -> String {
    match c {
        Curve::Linear => "linear".to_owned(),
        Curve::Log => "log".to_owned(),
        Curve::Stepped(n) => format!("stepped ({n})"),
    }
}

fn routing_label(r: FxRouting) -> &'static str {
    match r {
        FxRouting::Insert => "insert",
        FxRouting::SendReturn => "send/return",
    }
}

/// Renders the markdown effect tables, including the begin and end markers.
/// `docs/fx-catalog.md` embeds this text verbatim.
pub fn render_markdown() -> String {
    let mut out = String::new();
    out.push_str(DOC_BEGIN);
    out.push('\n');
    for e in CATALOG {
        out.push_str(&format!("\n### {} (`builtin:{}`)\n\n", e.name, e.id));
        out.push_str(e.summary);
        out.push_str("\n\n");
        out.push_str(&format!(
            "- Category: {}. Primary parameter (LEVEL/DEPTH target): `{}`.\n",
            if e.release {
                "release effect"
            } else {
                "normal effect"
            },
            e.primary
        ));
        out.push_str(&format!(
            "- Routing: {}. Tail: {}. Latency: {} frames.\n",
            routing_label(e.routing),
            if e.tail { "yes" } else { "no" },
            e.latency
        ));
        match e.sync {
            Some(s) => out.push_str(&format!(
                "- Beat sync: {}. Accepted division {} to {} beats, factory default {}.\n",
                s.meaning, s.min_beats, s.max_beats, s.default_beats
            )),
            None => out.push_str("- Beat sync: none.\n"),
        }
        out.push_str("\n| Id | Key | Name | Unit | Min | Max | Default | Curve |\n");
        out.push_str("|---:|---|---|---|---:|---:|---:|---|\n");
        for (i, q) in e.params.iter().enumerate() {
            out.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} | {} | {} |\n",
                i,
                q.key,
                q.name,
                unit_label(q.unit),
                q.min,
                q.max,
                q.default,
                curve_label(q.curve)
            ));
        }
        out.push_str("\nFactory presets: ");
        let names: Vec<String> = e
            .presets
            .iter()
            .map(|pr| format!("{} (wet {}, division {})", pr.name, pr.wet, pr.division))
            .collect();
        out.push_str(&names.join("; "));
        out.push_str(".\n");
    }
    out.push('\n');
    out.push_str(DOC_END);
    out.push('\n');
    out
}
