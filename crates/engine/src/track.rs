//! Track buffers handed to the audio thread.
//!
//! The engine never decodes. The loader thread (media) produces a
//! [`TrackBuffer`]: interleaved stereo `f32` at the engine sample rate. It
//! reaches the audio thread over a dedicated hand-off ring and comes back on
//! a return ring so the allocator is only ever touched off the audio thread
//! (ADR 0003, deferred free).

use std::sync::Arc;

use opendeck_contracts::{DeckId, Frames};

/// Why a [`TrackBuffer`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TrackError {
    /// The sample count is not a multiple of two (interleaved stereo).
    #[error("track sample count {0} is not a multiple of 2 (interleaved stereo)")]
    OddSampleCount(usize),
    /// The track has no frames.
    #[error("track has no frames")]
    Empty,
}

/// A fully decoded, resampled track: interleaved stereo `f32` at the engine
/// sample rate. Cloning is cheap (shared, immutable).
#[derive(Debug, Clone)]
pub struct TrackBuffer {
    samples: Arc<[f32]>,
}

impl TrackBuffer {
    /// Wraps interleaved stereo samples.
    pub fn new(samples: Arc<[f32]>) -> Result<Self, TrackError> {
        if samples.is_empty() {
            return Err(TrackError::Empty);
        }
        if !samples.len().is_multiple_of(2) {
            return Err(TrackError::OddSampleCount(samples.len()));
        }
        Ok(Self { samples })
    }

    /// Wraps a `Vec` of interleaved stereo samples.
    pub fn from_vec(samples: Vec<f32>) -> Result<Self, TrackError> {
        Self::new(samples.into())
    }

    /// Length in stereo frames.
    pub fn frames(&self) -> u64 {
        (self.samples.len() / 2) as u64
    }

    /// Length as [`Frames`].
    pub fn duration(&self) -> Frames {
        Frames(self.frames())
    }

    /// The interleaved samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

/// Heap objects travelling control -> audio on the hand-off ring.
///
/// Engine-private: callers use [`EngineHandle::load_track`](crate::EngineHandle::load_track).
#[derive(Debug)]
pub(crate) enum Handoff {
    /// Load `track` into `deck`.
    Track {
        deck: DeckId,
        track: TrackBuffer,
        /// Initial cue point, `None` for the start of the track.
        cue_point: Option<Frames>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_buffers() {
        assert_eq!(
            TrackBuffer::from_vec(vec![]).unwrap_err(),
            TrackError::Empty
        );
        assert_eq!(
            TrackBuffer::from_vec(vec![0.0; 3]).unwrap_err(),
            TrackError::OddSampleCount(3)
        );
        let t = TrackBuffer::from_vec(vec![0.0; 8]).expect("valid");
        assert_eq!(t.frames(), 4);
        assert_eq!(t.duration(), Frames(4));
    }
}
