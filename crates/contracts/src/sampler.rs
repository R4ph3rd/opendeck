//! Sampler types: play modes and per-slot telemetry.
//!
//! The sampler has [`SAMPLER_SLOTS`](crate::ids::SAMPLER_SLOTS) slots. A bank
//! is a UI and app-core grouping: the DDJ-FLX4 shows 8 pads per deck, so the
//! default layout is 2 banks of 8 (slots 0-7 for deck 1 pads, 8-15 for deck 2;
//! the operating manual, p.62, maps pads 1-8 to slots 1-8 or 9-16). The engine
//! knows only slots.

/// What a slot does when its pad is pressed and released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SamplerMode {
    /// Plays to the end; release is ignored. Pressing again restarts it.
    #[default]
    OneShot,
    /// Plays while the pad is held and stops on release.
    Gate,
    /// Repeats until stopped. Pressing again restarts it.
    Loop,
}

/// One sampler slot as the audio thread saw it at the end of a block. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SamplerSlotTelemetry {
    /// A sample is loaded.
    pub loaded: bool,
    /// The slot is sounding. A start waiting for the next beat is not playing yet.
    pub playing: bool,
    /// Position inside the sample, `[0, 1)`; 0 when not playing. A looping
    /// slot wraps from 1 back to 0.
    pub progress: f32,
}
