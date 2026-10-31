//! OpenDeck contracts: the cross-crate types and traits every other crate builds on.
//!
//! This crate sits at the bottom of the dependency graph (ADR 0002). It has no
//! internal dependencies and no required external ones. Any change here is a
//! contract change and goes through the architect.
//!
//! # Conventions
//!
//! - **Time.** Persisted and IPC positions (cues, beat grids, loops in the
//!   library) are [`Seconds`](units::Seconds) from the start of the decoded
//!   file, independent of sample rate. Engine-side positions
//!   ([`EngineCommand`](command::EngineCommand), [`EngineTelemetry`](telemetry::EngineTelemetry))
//!   are [`Frames`](units::Frames) at the engine (device) sample rate.
//! - **Gain.** Linear amplitude factors ([`Gain`](units::Gain)); dB only at
//!   the edges (UI labels, presets) via the helpers in [`units`].
//! - **Real-time.** Types marked *RT-safe* are `Copy`, own no heap memory and
//!   can cross the audio-thread rings. Methods marked *RT-safe* never
//!   allocate, lock, block, do I/O or panic on valid input.
//! - **Features.** `serde` adds serde derives; `specta` adds `specta::Type`
//!   derives for IPC DTOs and implies `serde`. The engine builds with neither.

pub mod analysis;
pub mod beatgrid;
pub mod command;
pub mod control;
pub mod cue;
pub mod deck;
pub mod fx;
pub mod ids;
pub mod jobs;
pub mod key;
pub mod mixer;
pub mod pad;
pub mod peaks;
pub mod sampler;
pub mod settings;
pub mod telemetry;
pub mod tempo;
pub mod units;

pub use command::EngineCommand;
pub use ids::{CueId, DeckId, FxSlotId, FxUnit, JobId, PlaylistId, SamplerSlotId, TrackId};
pub use telemetry::{EngineEvent, EngineTelemetry, FxUnitTelemetry};
pub use units::{Beats, Bpm, Frames, Gain, SampleRate, Seconds};

/// Maximum number of decks the architecture supports. The MVP UI shows two.
pub const MAX_DECKS: usize = 4;

/// Number of decks the MVP UI shows and the DDJ-FLX4 controls.
pub const MVP_DECKS: usize = 2;

/// Hot cues per deck (one per performance pad in HOT CUE mode).
pub const HOT_CUES_PER_DECK: usize = 8;

/// Performance pads per deck on the DDJ-FLX4 (message list section 5-5,
/// "PERFORMANCE PAD 1" to "PERFORMANCE PAD 8").
pub const PADS_PER_DECK: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_limits_are_consistent() {
        assert_eq!(MAX_DECKS, 4);
        const { assert!(MVP_DECKS <= MAX_DECKS) };
        assert_eq!(HOT_CUES_PER_DECK, PADS_PER_DECK);
    }
}
