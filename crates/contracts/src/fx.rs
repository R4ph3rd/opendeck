//! Effect processing contract and the plugin host boundary (ADR 0006).
//!
//! - [`Effect`] is one effect instance. The engine owns it on the audio thread
//!   and calls [`Effect::process`] once per block.
//! - [`EffectFactory`] creates instances. Built-in effects (`dsp-fx`) are one
//!   factory; a future VST3/CLAP host crate is another. The engine depends on
//!   neither: it only sees `Box<dyn Effect>`.
//! - [`EffectRegistry`] collects factories on the control thread.
//!
//! Wet/dry mix, bypass crossfades and latency compensation are done by the
//! engine's slot host, not by effects, so third-party plugins behave the same
//! as built-ins. An effect always writes its fully wet signal.
//!
//! Lifecycle: `create` (control thread) -> `prepare` (control thread, may
//! allocate) -> move to the audio thread over a ring -> `set_parameter` /
//! `process` / `reset` (audio thread, RT-safe) -> moved back over the return
//! ring -> dropped on the control thread.

use crate::units::{Beats, Bpm, SampleRate};

/// Parameter identifier, unique within one effect. Stable across versions of
/// that effect because presets store it. `u32` to fit CLAP and VST3 ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ParamId(pub u32);

/// Unit shown next to a parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ParamUnit {
    /// Plain number.
    None,
    /// Percent, 0..100.
    Percent,
    /// Decibels.
    Decibels,
    /// Hertz.
    Hertz,
    /// Milliseconds.
    Milliseconds,
    /// Beats (beat-synced time).
    Beats,
    /// Semitones.
    Semitones,
}

/// How a normalized `[0, 1]` value maps to the plain range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ParamCurve {
    /// `plain = min + n * (max - min)`.
    Linear,
    /// `plain = min * (max / min)^n`. Requires `min > 0` (frequencies, times).
    Logarithmic,
    /// `steps` evenly spaced values from `min` to `max`, `steps >= 2`
    /// (switches, beat divisions).
    Stepped {
        /// Number of distinct values.
        steps: u16,
    },
}

/// Static description of one parameter. Built on the control thread.
///
/// Invariants: `min < max`; `min <= default <= max`; for `Logarithmic`,
/// `min > 0`; for `Stepped`, `steps >= 2`. Values crossing the engine
/// boundary are normalized `[0, 1]`; [`ParamDescriptor::to_plain`] and
/// [`ParamDescriptor::to_normalized`] convert, RT-safe.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ParamDescriptor {
    /// Stable id.
    pub id: ParamId,
    /// Display name.
    pub name: String,
    /// Display unit.
    pub unit: ParamUnit,
    /// Lowest plain value.
    pub min: f32,
    /// Highest plain value.
    pub max: f32,
    /// Default plain value.
    pub default: f32,
    /// Mapping from normalized to plain.
    pub curve: ParamCurve,
}

impl ParamDescriptor {
    /// Normalized `[0, 1]` (clamped) to plain value. RT-safe.
    pub fn to_plain(&self, normalized: f32) -> f32 {
        let n = normalized.clamp(0.0, 1.0);
        match self.curve {
            ParamCurve::Linear => self.min + n * (self.max - self.min),
            ParamCurve::Logarithmic => self.min * (self.max / self.min).powf(n),
            ParamCurve::Stepped { steps } => {
                let last = f32::from(steps.max(2) - 1);
                let step = (n * last).round();
                self.min + step / last * (self.max - self.min)
            }
        }
    }

    /// Plain value (clamped to the range) to normalized `[0, 1]`. RT-safe.
    pub fn to_normalized(&self, plain: f32) -> f32 {
        let p = plain.clamp(self.min, self.max);
        let n = match self.curve {
            ParamCurve::Linear | ParamCurve::Stepped { .. } => {
                (p - self.min) / (self.max - self.min)
            }
            ParamCurve::Logarithmic => (p / self.min).ln() / (self.max / self.min).ln(),
        };
        n.clamp(0.0, 1.0)
    }

    /// Default as a normalized value.
    pub fn default_normalized(&self) -> f32 {
        self.to_normalized(self.default)
    }

    /// Checks the documented invariants.
    pub fn is_valid(&self) -> bool {
        let range_ok = self.min < self.max && (self.min..=self.max).contains(&self.default);
        let curve_ok = match self.curve {
            ParamCurve::Linear => true,
            ParamCurve::Logarithmic => self.min > 0.0,
            ParamCurve::Stepped { steps } => steps >= 2,
        };
        range_ok && curve_ok
    }
}

/// Where an effect is meant to sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum FxRouting {
    /// In series in a channel or master chain.
    Insert,
    /// On a send/return bus (reverbs, delays fed by sends).
    SendReturn,
}

/// Globally unique effect type: provider plus the provider's effect id.
/// Persisted in presets as `"provider:id"`-like pairs; both parts are stable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EffectKey {
    /// Factory provider id (`"builtin"`, later `"clap"`, `"vst3"`).
    pub provider: String,
    /// Effect id inside the provider (built-ins: snake_case name; plugins: plugin id).
    pub id: String,
}

/// Static description of an effect type.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EffectInfo {
    /// Unique key.
    pub key: EffectKey,
    /// Display name.
    pub name: String,
    /// Intended routing.
    pub routing: FxRouting,
    /// True if the effect reads `ProcessContext::beat_division`.
    pub beat_synced: bool,
    /// True if output continues after input stops (echo, reverb); the host
    /// keeps processing after bypass until the tail decays.
    pub has_tail: bool,
    /// Parameters, in display order.
    pub params: Vec<ParamDescriptor>,
}

/// A saved effect setting (`fx_presets` table). Values are normalized, keyed
/// by [`ParamId`]; ids the effect no longer has are ignored on recall.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FxPreset {
    /// Effect type.
    pub effect: EffectKey,
    /// Preset name, unique per effect type.
    pub name: String,
    /// Normalized parameter values.
    pub params: Vec<(ParamId, f32)>,
    /// Wet/dry mix, `[0, 1]`.
    pub wet: f32,
    /// Beat division, for beat-synced effects.
    pub beat_division: Beats,
}

/// Musical and transport context for one block. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessContext {
    /// Engine sample rate (same as passed to `prepare`).
    pub sample_rate: SampleRate,
    /// Tempo of the deck feeding the slot (or the leader for master/send), if known.
    pub bpm: Option<Bpm>,
    /// Beat position at the first frame of the block, if known. Lets
    /// beat-synced effects lock their phase to the grid.
    pub beat_position: Option<Beats>,
    /// Selected beat division for the slot (e.g. `0.5` = 1/2 beat).
    pub beat_division: Beats,
    /// True when the source is playing.
    pub playing: bool,
}

/// In-place stereo block, planar `f32`. RT-safe.
///
/// Invariant: `left.len() == right.len() <= max_block_frames` given to
/// `prepare`. Enforced by [`StereoBufferMut::new`].
#[derive(Debug)]
pub struct StereoBufferMut<'a> {
    left: &'a mut [f32],
    right: &'a mut [f32],
}

impl<'a> StereoBufferMut<'a> {
    /// Wraps two channel slices; `None` if their lengths differ.
    pub fn new(left: &'a mut [f32], right: &'a mut [f32]) -> Option<Self> {
        (left.len() == right.len()).then_some(Self { left, right })
    }

    /// Frames in the block.
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    /// Both channels, mutable.
    pub fn channels(&mut self) -> (&mut [f32], &mut [f32]) {
        (self.left, self.right)
    }
}

/// One effect instance.
///
/// Real-time rules: [`Effect::set_parameter`], [`Effect::process`],
/// [`Effect::reset`], [`Effect::latency_frames`] and [`Effect::info`] run on the
/// audio thread and must not allocate, free, lock, block, do I/O, log or
/// panic. All memory is acquired in [`Effect::prepare`]. `info` returns a
/// reference to data built before the instance reached the audio thread.
pub trait Effect: Send {
    /// Static description. RT-safe (returns a stored reference).
    fn info(&self) -> &EffectInfo;

    /// Allocates buffers for `sample_rate` and blocks of up to
    /// `max_block_frames`. Called on the control thread before the instance
    /// is handed to the audio thread, and again (off the audio thread) if the
    /// stream configuration changes. May allocate.
    fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize);

    /// Clears internal state (delay lines, filters) without freeing memory. RT-safe.
    fn reset(&mut self);

    /// Sets a parameter from a normalized `[0, 1]` value. Unknown ids are
    /// ignored. The effect smooths internally if needed. RT-safe.
    fn set_parameter(&mut self, id: ParamId, normalized: f32);

    /// Processes one block in place, writing the fully wet signal. RT-safe.
    fn process(&mut self, io: StereoBufferMut<'_>, ctx: &ProcessContext);

    /// Processing latency in frames at the prepared rate, for delay
    /// compensation by the host. RT-safe. Default 0.
    fn latency_frames(&self) -> u32 {
        0
    }
}

/// Error creating an effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectError {
    /// No registered provider with this id.
    UnknownProvider(String),
    /// The provider has no effect with this id.
    UnknownEffect(EffectKey),
    /// A provider with this id is already registered.
    DuplicateProvider(String),
    /// The provider failed to instantiate the effect (plugin load error etc.).
    Failed(String),
}

impl core::fmt::Display for EffectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownProvider(p) => write!(f, "unknown effect provider {p}"),
            Self::UnknownEffect(k) => write!(f, "unknown effect {}:{}", k.provider, k.id),
            Self::DuplicateProvider(p) => write!(f, "effect provider {p} registered twice"),
            Self::Failed(msg) => write!(f, "effect creation failed: {msg}"),
        }
    }
}

impl std::error::Error for EffectError {}

/// Creates effect instances of one provider. Control thread only; may
/// allocate, load libraries and block.
pub trait EffectFactory: Send + Sync {
    /// Provider id, unique in the registry (`"builtin"`).
    fn provider(&self) -> &str;

    /// Effects this provider offers.
    fn effects(&self) -> Vec<EffectInfo>;

    /// Creates an unprepared instance of effect `id`.
    fn create(&self, id: &str) -> Result<Box<dyn Effect>, EffectError>;
}

/// All registered effect factories. Lives in app-core; control thread only.
#[derive(Default)]
pub struct EffectRegistry {
    factories: Vec<Box<dyn EffectFactory>>,
}

impl EffectRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a factory. Fails if its provider id is already registered.
    pub fn register(&mut self, factory: Box<dyn EffectFactory>) -> Result<(), EffectError> {
        if self
            .factories
            .iter()
            .any(|f| f.provider() == factory.provider())
        {
            return Err(EffectError::DuplicateProvider(
                factory.provider().to_owned(),
            ));
        }
        self.factories.push(factory);
        Ok(())
    }

    /// Every effect of every provider, in registration order.
    pub fn effects(&self) -> Vec<EffectInfo> {
        self.factories.iter().flat_map(|f| f.effects()).collect()
    }

    /// Creates an unprepared instance.
    pub fn create(&self, key: &EffectKey) -> Result<Box<dyn Effect>, EffectError> {
        let factory = self
            .factories
            .iter()
            .find(|f| f.provider() == key.provider)
            .ok_or_else(|| EffectError::UnknownProvider(key.provider.clone()))?;
        factory.create(&key.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Gain {
        info: EffectInfo,
        gain: f32,
    }

    impl Effect for Gain {
        fn info(&self) -> &EffectInfo {
            &self.info
        }
        fn prepare(&mut self, _: SampleRate, _: usize) {}
        fn reset(&mut self) {}
        fn set_parameter(&mut self, id: ParamId, normalized: f32) {
            if id == ParamId(0) {
                self.gain = self.info.params[0].to_plain(normalized);
            }
        }
        fn process(&mut self, mut io: StereoBufferMut<'_>, _: &ProcessContext) {
            let (l, r) = io.channels();
            for s in l.iter_mut().chain(r.iter_mut()) {
                *s *= self.gain;
            }
        }
    }

    fn gain_info() -> EffectInfo {
        EffectInfo {
            key: EffectKey {
                provider: "test".into(),
                id: "gain".into(),
            },
            name: "Gain".into(),
            routing: FxRouting::Insert,
            beat_synced: false,
            has_tail: false,
            params: vec![ParamDescriptor {
                id: ParamId(0),
                name: "Gain".into(),
                unit: ParamUnit::None,
                min: 0.0,
                max: 2.0,
                default: 1.0,
                curve: ParamCurve::Linear,
            }],
        }
    }

    struct TestFactory;

    impl EffectFactory for TestFactory {
        fn provider(&self) -> &str {
            "test"
        }
        fn effects(&self) -> Vec<EffectInfo> {
            vec![gain_info()]
        }
        fn create(&self, id: &str) -> Result<Box<dyn Effect>, EffectError> {
            match id {
                "gain" => Ok(Box::new(Gain {
                    info: gain_info(),
                    gain: 1.0,
                })),
                _ => Err(EffectError::UnknownEffect(EffectKey {
                    provider: "test".into(),
                    id: id.into(),
                })),
            }
        }
    }

    #[test]
    fn registry_creates_and_processes() {
        let mut registry = EffectRegistry::new();
        registry.register(Box::new(TestFactory)).expect("first");
        assert!(registry.register(Box::new(TestFactory)).is_err());
        assert_eq!(registry.effects().len(), 1);

        let key = gain_info().key;
        let mut fx = registry.create(&key).expect("known effect");
        fx.prepare(SampleRate(48_000), 4);
        fx.set_parameter(ParamId(0), 0.25);
        let mut l = [1.0_f32; 4];
        let mut r = [1.0_f32; 4];
        let ctx = ProcessContext {
            sample_rate: SampleRate(48_000),
            bpm: None,
            beat_position: None,
            beat_division: Beats(1.0),
            playing: true,
        };
        fx.process(
            StereoBufferMut::new(&mut l, &mut r).expect("same len"),
            &ctx,
        );
        assert_eq!(l, [0.5; 4]);
        assert_eq!(fx.latency_frames(), 0);

        let missing = EffectKey {
            provider: "nope".into(),
            id: "x".into(),
        };
        assert!(matches!(
            registry.create(&missing),
            Err(EffectError::UnknownProvider(_))
        ));
    }

    #[test]
    fn param_curves() {
        let mut p = gain_info().params.remove(0);
        assert!(p.is_valid());
        assert_eq!(p.to_plain(0.5), 1.0);
        assert_eq!(p.default_normalized(), 0.5);
        p.curve = ParamCurve::Logarithmic;
        p.min = 20.0;
        p.max = 20_000.0;
        p.default = 1000.0;
        assert!((p.to_plain(0.5) - 632.455_5).abs() < 0.01);
        assert!((p.to_normalized(632.455_5) - 0.5).abs() < 1e-4);
        p.curve = ParamCurve::Stepped { steps: 3 };
        p.min = 0.0;
        p.max = 2.0;
        p.default = 1.0;
        assert_eq!(p.to_plain(0.4), 1.0);
        assert_eq!(p.to_plain(0.9), 2.0);
    }

    #[test]
    fn mismatched_buffers_rejected() {
        let mut l = [0.0_f32; 3];
        let mut r = [0.0_f32; 4];
        assert!(StereoBufferMut::new(&mut l, &mut r).is_none());
    }
}
