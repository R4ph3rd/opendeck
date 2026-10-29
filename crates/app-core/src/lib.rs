//! OpenDeck application core.
//!
//! Top of the crate graph (ADR 0002): wires the engine, library, analysis,
//! effects and MIDI together, runs the control thread and owns application
//! state (decks, pad modes, settings). The Tauri shell talks only to this
//! crate. See `CONTEXT/app-core.md`.

use opendeck_contracts::settings::{AudioHostInfo, AudioHostKind};
use opendeck_contracts::{DeckId, MAX_DECKS, pad::PadMode};

/// Human-readable product name.
pub const PRODUCT_NAME: &str = "OpenDeck";

/// Crate version, shared by every workspace member.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application state owned by the control side. Not touched by the audio
/// thread. The shell wraps it in a mutex until the control thread lands
/// (milestone 1); commands then become messages to that thread.
#[derive(Debug, Clone)]
pub struct AppCore {
    pad_modes: [PadMode; MAX_DECKS],
}

impl Default for AppCore {
    fn default() -> Self {
        Self::new()
    }
}

impl AppCore {
    /// Fresh state: every deck in HOT CUE mode.
    pub fn new() -> Self {
        Self {
            pad_modes: [PadMode::HotCue; MAX_DECKS],
        }
    }

    /// Active pad mode of `deck`. Drives the pads and the middle UI panel.
    pub fn pad_mode(&self, deck: DeckId) -> PadMode {
        self.pad_modes[deck.index()]
    }

    /// Sets the pad mode of `deck` (from a MIDI mode button or a UI click).
    /// Returns true if it changed, so the caller emits a change event and LED
    /// feedback only when needed.
    pub fn set_pad_mode(&mut self, deck: DeckId, mode: PadMode) -> bool {
        let slot = &mut self.pad_modes[deck.index()];
        let changed = *slot != mode;
        *slot = mode;
        changed
    }

    /// Audio host APIs for the settings screen.
    ///
    /// Stub: reports the hosts this platform build is expected to offer. The
    /// engine replaces it with real cpal enumeration in milestone 1.
    pub fn audio_hosts(&self) -> Vec<AudioHostInfo> {
        let host = |kind, name: &str, available, is_default| AudioHostInfo {
            kind,
            name: name.to_owned(),
            available,
            is_default,
        };
        if cfg!(target_os = "windows") {
            vec![
                host(AudioHostKind::Wasapi, "WASAPI", true, true),
                host(AudioHostKind::Asio, "ASIO", false, false),
            ]
        } else if cfg!(target_os = "macos") {
            vec![host(AudioHostKind::CoreAudio, "Core Audio", true, true)]
        } else {
            vec![
                host(AudioHostKind::Alsa, "ALSA", true, true),
                host(AudioHostKind::Jack, "JACK", false, false),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_workspace() {
        assert_eq!(VERSION, "0.0.0");
        assert_eq!(PRODUCT_NAME, "OpenDeck");
    }

    #[test]
    fn pad_mode_per_deck() {
        let mut core = AppCore::new();
        let deck2 = DeckId::new(1).expect("deck 2");
        assert_eq!(core.pad_mode(deck2), PadMode::HotCue);
        assert!(core.set_pad_mode(deck2, PadMode::Sampler));
        assert!(!core.set_pad_mode(deck2, PadMode::Sampler));
        assert_eq!(core.pad_mode(deck2), PadMode::Sampler);
        let deck1 = DeckId::new(0).expect("deck 1");
        assert_eq!(core.pad_mode(deck1), PadMode::HotCue);
    }

    #[test]
    fn exactly_one_default_host() {
        let hosts = AppCore::new().audio_hosts();
        assert_eq!(hosts.iter().filter(|h| h.is_default).count(), 1);
    }
}
