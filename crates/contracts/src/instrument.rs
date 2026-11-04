//! Instrument contract: a note-driven sound source (ADR 0008).
//!
//! An [`Instrument`] is the counterpart of [`Effect`](crate::fx::Effect) for
//! sources: it has no audio input, receives [`NoteEvent`]s and writes a stereo
//! block. CLAP and VST3 instruments (`crates/plugin-host`) implement it; the
//! engine hosts instances in instrument slots triggered by pads and MIDI
//! notes (milestone 9a-4). The engine never sees which provider made one.
//!
//! Lifecycle and threading are the same as effects (ADR 0006): `create`
//! (control thread) -> `prepare` (off the audio thread, may allocate) -> hand
//! off to the audio thread -> `set_parameter` / `process` / `reset` (audio
//! thread, RT-safe) -> return ring -> dropped off the audio thread.

use crate::fx::{
    EffectError, EffectKey, ParamDescriptor, ParamId, ProcessContext, StereoBufferMut,
};
use crate::units::SampleRate;

/// Most note events one instrument receives in one block. Engine queues and
/// plugin-host translation buffers are this size; events past it in a block
/// are dropped by the host (note-offs are kept before note-ons, see
/// [`NoteEvent`]).
pub const MAX_NOTE_EVENTS: usize = 128;

/// What a [`NoteEvent`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteKind {
    /// Key pressed.
    On,
    /// Key released. The voice may keep sounding (release stage).
    Off,
}

/// One note event inside a block. RT-safe (`Copy`, no heap).
///
/// Invariants, enforced by the constructors: `channel < 16`, `key < 128`
/// (MIDI numbering, 60 = C4, 69 = A4 at 440 Hz), `velocity` finite and in
/// `[0, 1]` (MIDI velocity `v` maps to `v / 127`). `offset` is the frame
/// inside the block, `offset < frames` of the block it is delivered with;
/// hosts deliver events sorted by `offset`, and an `Off` before an `On` at
/// the same offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteEvent {
    offset: u32,
    channel: u8,
    key: u8,
    velocity: f32,
    kind: NoteKind,
}

impl NoteEvent {
    /// Builds an event; `None` if `channel >= 16`, `key >= 128` or
    /// `velocity` is not finite. Velocity is clamped to `[0, 1]`. RT-safe.
    pub fn new(offset: u32, kind: NoteKind, channel: u8, key: u8, velocity: f32) -> Option<Self> {
        (channel < 16 && key < 128 && velocity.is_finite()).then_some(Self {
            offset,
            channel,
            key,
            velocity: velocity.clamp(0.0, 1.0),
            kind,
        })
    }

    /// Note-on at `offset`. See [`NoteEvent::new`]. RT-safe.
    pub fn on(offset: u32, channel: u8, key: u8, velocity: f32) -> Option<Self> {
        Self::new(offset, NoteKind::On, channel, key, velocity)
    }

    /// Note-off at `offset`. See [`NoteEvent::new`]. RT-safe.
    pub fn off(offset: u32, channel: u8, key: u8, velocity: f32) -> Option<Self> {
        Self::new(offset, NoteKind::Off, channel, key, velocity)
    }

    /// Frame inside the block.
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// MIDI channel, `0..16`.
    pub fn channel(&self) -> u8 {
        self.channel
    }

    /// MIDI key, `0..128`.
    pub fn key(&self) -> u8 {
        self.key
    }

    /// Velocity, `[0, 1]`.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    /// On or off.
    pub fn kind(&self) -> NoteKind {
        self.kind
    }

    /// Same event moved to another frame of the block (hosts splitting or
    /// merging blocks). RT-safe.
    pub fn with_offset(self, offset: u32) -> Self {
        Self { offset, ..self }
    }
}

/// Static description of an instrument type. Built on the control thread.
///
/// `key` uses the same provider/id scheme as effects (`"clap"`, `"vst3"`);
/// an effect and an instrument never share a key.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InstrumentInfo {
    /// Unique key.
    pub key: EffectKey,
    /// Display name.
    pub name: String,
    /// Parameters, in display order. Same rules as effect parameters.
    pub params: Vec<ParamDescriptor>,
}

/// One instrument instance.
///
/// Real-time rules are those of [`Effect`](crate::fx::Effect):
/// [`Instrument::set_parameter`], [`Instrument::process`],
/// [`Instrument::reset`], [`Instrument::latency_frames`] and
/// [`Instrument::info`] run on the audio thread and must not allocate, free,
/// lock, block, do I/O, log or panic. All memory is acquired in
/// [`Instrument::prepare`].
pub trait Instrument: Send {
    /// Static description. RT-safe (returns a stored reference).
    fn info(&self) -> &InstrumentInfo;

    /// Allocates for `sample_rate` and blocks of up to `max_block_frames`.
    /// Off the audio thread; called again when the stream configuration
    /// changes. May allocate and block.
    fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize);

    /// Silences the instrument at once: every voice stops, tails are
    /// cleared, no memory is freed. Used for panic and slot reuse. RT-safe.
    fn reset(&mut self);

    /// Sets a parameter from a normalized `[0, 1]` value. Unknown ids are
    /// ignored. RT-safe.
    fn set_parameter(&mut self, id: ParamId, normalized: f32);

    /// Renders one block into `out`, overwriting it (the host does not clear
    /// it first). `notes` are this block's events, sorted by offset, at most
    /// [`MAX_NOTE_EVENTS`], every `offset < out.frames()`. RT-safe.
    ///
    /// `ctx.beat_division` has no meaning for instruments and is ignored.
    fn process(&mut self, out: StereoBufferMut<'_>, notes: &[NoteEvent], ctx: &ProcessContext);

    /// Output latency in frames at the prepared rate. RT-safe. Default 0.
    fn latency_frames(&self) -> u32 {
        0
    }
}

/// Creates instrument instances of one provider. Control thread only; may
/// allocate, load libraries and block. Errors reuse [`EffectError`].
pub trait InstrumentFactory: Send + Sync {
    /// Provider id, unique among instrument factories (`"clap"`, `"vst3"`).
    fn provider(&self) -> &str;

    /// Instruments this provider offers.
    fn instruments(&self) -> Vec<InstrumentInfo>;

    /// Creates an unprepared instance of instrument `id`.
    fn create(&self, id: &str) -> Result<Box<dyn Instrument>, EffectError>;
}

/// All registered instrument factories. Lives in app-core; control thread only.
#[derive(Default)]
pub struct InstrumentRegistry {
    factories: Vec<Box<dyn InstrumentFactory>>,
}

impl InstrumentRegistry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a factory. Fails if its provider id is already registered.
    pub fn register(&mut self, factory: Box<dyn InstrumentFactory>) -> Result<(), EffectError> {
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

    /// Every instrument of every provider, in registration order.
    pub fn instruments(&self) -> Vec<InstrumentInfo> {
        self.factories
            .iter()
            .flat_map(|f| f.instruments())
            .collect()
    }

    /// Creates an unprepared instance.
    pub fn create(&self, key: &EffectKey) -> Result<Box<dyn Instrument>, EffectError> {
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
    use crate::units::Beats;

    #[test]
    fn note_event_validates_and_is_plain_data() {
        const { assert!(!core::mem::needs_drop::<NoteEvent>()) };
        assert!(core::mem::size_of::<NoteEvent>() <= 16);
        assert!(NoteEvent::on(0, 16, 60, 1.0).is_none());
        assert!(NoteEvent::on(0, 0, 128, 1.0).is_none());
        assert!(NoteEvent::on(0, 0, 60, f32::NAN).is_none());
        let n = NoteEvent::on(3, 9, 36, 1.5).expect("valid");
        assert_eq!(
            (n.offset(), n.channel(), n.key(), n.velocity()),
            (3, 9, 36, 1.0)
        );
        assert_eq!(n.kind(), NoteKind::On);
        assert_eq!(n.with_offset(7).offset(), 7);
        assert_eq!(
            NoteEvent::off(0, 0, 60, -1.0).map(|e| e.velocity()),
            Some(0.0)
        );
    }

    struct Click {
        info: InstrumentInfo,
    }

    impl Instrument for Click {
        fn info(&self) -> &InstrumentInfo {
            &self.info
        }
        fn prepare(&mut self, _: SampleRate, _: usize) {}
        fn reset(&mut self) {}
        fn set_parameter(&mut self, _: ParamId, _: f32) {}
        fn process(
            &mut self,
            mut out: StereoBufferMut<'_>,
            notes: &[NoteEvent],
            _: &ProcessContext,
        ) {
            let (l, r) = out.channels();
            l.fill(0.0);
            r.fill(0.0);
            for n in notes.iter().filter(|n| n.kind() == NoteKind::On) {
                let i = n.offset() as usize;
                if let (Some(a), Some(b)) = (l.get_mut(i), r.get_mut(i)) {
                    *a = n.velocity();
                    *b = n.velocity();
                }
            }
        }
    }

    struct ClickFactory;

    fn click_info() -> InstrumentInfo {
        InstrumentInfo {
            key: EffectKey {
                provider: "test".into(),
                id: "click".into(),
            },
            name: "Click".into(),
            params: Vec::new(),
        }
    }

    impl InstrumentFactory for ClickFactory {
        fn provider(&self) -> &str {
            "test"
        }
        fn instruments(&self) -> Vec<InstrumentInfo> {
            vec![click_info()]
        }
        fn create(&self, id: &str) -> Result<Box<dyn Instrument>, EffectError> {
            match id {
                "click" => Ok(Box::new(Click { info: click_info() })),
                _ => Err(EffectError::UnknownEffect(EffectKey {
                    provider: "test".into(),
                    id: id.into(),
                })),
            }
        }
    }

    #[test]
    fn registry_creates_and_renders() {
        let mut registry = InstrumentRegistry::new();
        registry.register(Box::new(ClickFactory)).expect("first");
        assert!(registry.register(Box::new(ClickFactory)).is_err());
        assert_eq!(registry.instruments().len(), 1);
        let mut inst = registry.create(&click_info().key).expect("known");
        inst.prepare(SampleRate(48_000), 4);
        let mut l = [9.0_f32; 4];
        let mut r = [9.0_f32; 4];
        let ctx = ProcessContext {
            sample_rate: SampleRate(48_000),
            bpm: None,
            beat_position: None,
            beat_division: Beats(1.0),
            playing: false,
        };
        let notes = [NoteEvent::on(2, 0, 60, 0.5).expect("valid")];
        inst.process(
            StereoBufferMut::new(&mut l, &mut r).expect("same len"),
            &notes,
            &ctx,
        );
        assert_eq!(l, [0.0, 0.0, 0.5, 0.0]);
        assert_eq!(inst.latency_frames(), 0);
        assert!(matches!(
            registry.create(&EffectKey {
                provider: "nope".into(),
                id: "x".into()
            }),
            Err(EffectError::UnknownProvider(_))
        ));
    }
}
