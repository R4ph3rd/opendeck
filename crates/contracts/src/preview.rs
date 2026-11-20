//! Track preview (audition): telemetry of the preview voice.
//!
//! The preview voice is independent of the decks. A decoded buffer reaches it
//! over the engine's hand-off ring; `EngineCommand::PreviewPlay` and
//! `PreviewStop` drive it. It plays on the headphone cue bus, or on the master
//! at a reduced level when the device has no headphone pair
//! (`EngineCommand::SetPreviewToMaster`). See `CONTEXT/engine.md`.

use crate::units::Frames;

/// State of the preview voice at the end of a block. RT-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PreviewTelemetry {
    /// A buffer is loaded in the preview voice.
    pub loaded: bool,
    /// The voice is sounding. It turns `false` at the end of the track or
    /// after `PreviewStop`.
    pub playing: bool,
    /// Identifies the loaded buffer: the value the control side gave when it
    /// handed the buffer over. Lets the UI tell its own track from a previous
    /// one while a new buffer is still decoding. `0` when nothing was loaded.
    pub token: u32,
    /// Playhead in engine frames; 0 when nothing is loaded.
    pub position: Frames,
    /// Length of the loaded buffer in engine frames; 0 when nothing is loaded.
    pub duration: Frames,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_telemetry_is_copy_without_drop() {
        assert!(!core::mem::needs_drop::<PreviewTelemetry>());
        assert_eq!(PreviewTelemetry::default().token, 0);
    }
}
