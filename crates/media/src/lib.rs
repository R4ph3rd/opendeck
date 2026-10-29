//! OpenDeck media: file decoding (symphonia), resampling to the engine rate
//! (rubato) and tag and artwork reading (lofty).
//!
//! Shared by the engine loader thread and the analysis pipeline. Nothing in
//! this crate runs on the audio thread. See `CONTEXT/media.md`.

mod decode;
mod error;
mod tags;

pub use decode::{AudioView, DecodedAudio, decode_file};
pub use error::MediaError;
pub use tags::{Artwork, TrackTags, read_tags};

/// Lower-case file extensions the decoder is expected to accept (ADR 0001).
///
/// `m4a` covers both AAC and ALAC. Discovery filters files with this list
/// before probing, so the list must stay in sync with the enabled symphonia
/// codecs.
pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["mp3", "m4a", "aac", "flac", "wav", "aif", "aiff", "ogg"];

/// Returns true when `ext` (without the dot, any case) is a supported audio extension.
pub fn is_supported_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_check_is_case_insensitive() {
        assert!(is_supported_extension("FLAC"));
        assert!(is_supported_extension("m4a"));
        assert!(!is_supported_extension("txt"));
    }
}
