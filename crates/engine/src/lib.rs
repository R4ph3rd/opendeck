//! OpenDeck real-time audio engine: decks, mixer, routing, sync, loops,
//! time-stretch and an offline renderer for tests.
//!
//! The audio callback never allocates, locks, does I/O, logs or panics
//! (ADR 0003). It receives `EngineCommand`s over an SPSC ring and publishes
//! `EngineTelemetry` over another. See `CONTEXT/engine.md`.

use opendeck_contracts::MAX_DECKS;

/// Static engine configuration, fixed for the lifetime of an audio stream.
///
/// Changing any field means stopping the stream, re-preparing every deck and
/// effect, and starting a new stream. Never mutated on the audio thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    /// Device sample rate in Hz. All engine-side frame counts use this rate.
    pub sample_rate: u32,
    /// Largest block, in frames, the callback will be asked to render.
    /// Scratch buffers are preallocated to this size.
    pub max_block_frames: u32,
    /// Number of decks to allocate, `1..=MAX_DECKS`.
    pub deck_count: u8,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            max_block_frames: 1024,
            deck_count: 2,
        }
    }
}

impl EngineConfig {
    /// Returns true when every field is inside its documented range.
    pub fn is_valid(&self) -> bool {
        self.sample_rate > 0
            && self.max_block_frames > 0
            && (1..=MAX_DECKS).contains(&usize::from(self.deck_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_two_decks() {
        let config = EngineConfig::default();
        assert!(config.is_valid());
        assert_eq!(config.deck_count, 2);
    }

    #[test]
    fn rejects_too_many_decks() {
        let config = EngineConfig {
            deck_count: 5,
            ..EngineConfig::default()
        };
        assert!(!config.is_valid());
    }
}
