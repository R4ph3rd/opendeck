//! OpenDeck MIDI: device I/O (midir), data-driven JSON mappings that turn MIDI
//! messages into controller-agnostic `ControlAction`s, LED feedback, the MIDI
//! monitor and learn backend, and the replay harness.
//!
//! The only source for DDJ-FLX4 bytes is the Pioneer DJ message list
//! (`docs/sources/ddj-flx4-midi.txt`). Unconfirmed values are `TODO(verify)`.
//! See `CONTEXT/midi.md` and ADR 0007.

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
