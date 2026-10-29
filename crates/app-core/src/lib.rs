//! OpenDeck application core.
//!
//! Top of the crate graph (ADR 0002): wires the engine, library, analysis,
//! effects and MIDI together, runs the control thread and owns application
//! state (decks, pad modes, settings). The Tauri shell talks only to this
//! crate. See `CONTEXT/app-core.md`.

/// Human-readable product name.
pub const PRODUCT_NAME: &str = "OpenDeck";

/// Crate version, shared by every workspace member.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_workspace() {
        assert_eq!(VERSION, "0.0.0");
        assert_eq!(PRODUCT_NAME, "OpenDeck");
    }
}
