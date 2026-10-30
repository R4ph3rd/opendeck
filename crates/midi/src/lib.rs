//! OpenDeck MIDI: device I/O (midir), data-driven JSON mappings that turn MIDI
//! messages into controller-agnostic `ControlAction`s, LED feedback, the MIDI
//! monitor and learn backend, and the replay harness.
//!
//! The only source for DDJ-FLX4 bytes is the Pioneer DJ message list
//! (`docs/sources/ddj-flx4-midi.txt`). Unconfirmed values carry `verify` in
//! the mapping and a row in `docs/midi-open-questions.md`.
//! See `CONTEXT/midi.md` and ADR 0007.
//!
//! # Layers
//!
//! - [`mapping`]: the JSON mapping types, loading and validation.
//! - [`input`] and [`feedback`]: the two engines, pure state machines driven
//!   with explicit timestamps. [`pipeline::Pipeline`] wires them together.
//!
//! The crate never touches the audio thread and never depends on the engine.

pub mod error;
pub mod feedback;
pub mod input;
pub mod mapping;
pub mod message;
pub mod pipeline;

pub use error::{MappingError, MidiError};
pub use feedback::{FeedbackConfig, FeedbackEngine, OutMessage};
pub use input::{Handled, MappingEngine};
pub use mapping::Mapping;
pub use pipeline::Pipeline;

/// Current mapping file format version (`mappings/schema/mapping.schema.json`).
pub const MAPPING_FORMAT_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_format_version_is_one() {
        assert_eq!(MAPPING_FORMAT_VERSION, 1);
    }
}
