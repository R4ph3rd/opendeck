//! OpenDeck real-time audio engine: decks, mixer, routing, sync, loops,
//! time-stretch and an offline renderer for tests.
//!
//! The audio callback never allocates, locks, does I/O, logs or panics
//! (ADR 0003). It receives `EngineCommand`s over an SPSC ring and publishes
//! `EngineTelemetry` over another. See `CONTEXT/engine.md`.
//!
//! # Shape
//!
//! [`engine`] builds a pair: an [`EngineCore`] (moved to the audio thread or
//! an [`OfflineRenderer`]) and an [`EngineHandle`] (control thread). Tracks
//! arrive decoded as [`TrackBuffer`]s over a dedicated hand-off ring and are
//! returned on a return ring for freeing off the audio thread. The device
//! layer in [`device`] opens a cpal output stream around a core.

mod audio;
mod deck;
pub mod device;
mod handle;
mod meter;
mod offline;
mod track;

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests_heap;
#[cfg(test)]
mod tests_playback;

pub use crate::audio::EngineCore;
pub use crate::deck::{DeckStatus, Interpolation, MAX_PITCH_BEND, MAX_RATE};
pub use crate::handle::{
    COMMAND_RING, CommandRingFull, EVENT_RING, EngineHandle, HANDOFF_RING, RETURN_RING,
    TELEMETRY_RING, engine, engine_with_xruns,
};
pub use crate::offline::{OfflineRenderer, RenderStats};
pub use crate::track::{TrackBuffer, TrackError};

use opendeck_contracts::{MAX_DECKS, SampleRate};

/// Static engine configuration, fixed for the lifetime of an audio stream.
///
/// Changing any field means stopping the stream, re-preparing every deck and
/// effect, and starting a new stream. Never mutated on the audio thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    /// Device sample rate. All engine-side frame counts use this rate.
    pub sample_rate: SampleRate,
    /// Largest block, in frames, the callback will be asked to render.
    /// Scratch buffers are preallocated to this size; longer callbacks are
    /// rendered in chunks.
    pub max_block_frames: u32,
    /// Number of decks to allocate, `1..=MAX_DECKS`.
    pub deck_count: u8,
    /// Interpolation for plain varispeed playback (key lock off).
    pub interpolation: Interpolation,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: SampleRate::default(),
            max_block_frames: 1024,
            deck_count: 2,
            interpolation: Interpolation::default(),
        }
    }
}

/// An [`EngineConfig`] field is out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// The sample rate is zero.
    #[error("sample rate must be greater than zero")]
    SampleRate,
    /// The maximum block size is zero.
    #[error("max block frames must be greater than zero")]
    BlockFrames,
    /// The deck count is outside `1..=MAX_DECKS`.
    #[error("deck count must be in 1..={MAX_DECKS}")]
    DeckCount,
}

impl EngineConfig {
    /// Checks every field against its documented range.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.sample_rate.0 == 0 {
            return Err(ConfigError::SampleRate);
        }
        if self.max_block_frames == 0 {
            return Err(ConfigError::BlockFrames);
        }
        if !(1..=MAX_DECKS).contains(&usize::from(self.deck_count)) {
            return Err(ConfigError::DeckCount);
        }
        Ok(())
    }

    /// Returns true when every field is inside its documented range.
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

/// Runs `f` under `assert_no_alloc` when the `alloc-guard` feature or tests
/// are on, plainly otherwise. In an aborting build any allocation or free
/// inside `f` aborts the process.
#[inline]
pub(crate) fn rt_guard<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(any(test, feature = "alloc-guard"))]
    {
        assert_no_alloc::assert_no_alloc(f)
    }
    #[cfg(not(any(test, feature = "alloc-guard")))]
    {
        f()
    }
}

/// Test binaries abort on any allocation inside [`rt_guard`]. Release builds
/// compile `assert_no_alloc` out, so the allocator is debug-only.
#[cfg(all(test, debug_assertions))]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_two_decks() {
        let config = EngineConfig::default();
        assert!(config.is_valid());
        assert_eq!(config.deck_count, 2);
        assert_eq!(config.sample_rate, SampleRate(48_000));
    }

    #[test]
    fn rejects_out_of_range_fields() {
        let base = EngineConfig::default();
        let bad = |c: EngineConfig| c.validate().unwrap_err();
        assert_eq!(
            bad(EngineConfig {
                deck_count: 5,
                ..base
            }),
            ConfigError::DeckCount
        );
        assert_eq!(
            bad(EngineConfig {
                deck_count: 0,
                ..base
            }),
            ConfigError::DeckCount
        );
        assert_eq!(
            bad(EngineConfig {
                sample_rate: SampleRate(0),
                ..base
            }),
            ConfigError::SampleRate
        );
        assert_eq!(
            bad(EngineConfig {
                max_block_frames: 0,
                ..base
            }),
            ConfigError::BlockFrames
        );
    }
}
