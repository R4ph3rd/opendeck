//! Typed errors for decoding and tag reading.

use std::path::PathBuf;

/// Failure while decoding, resampling or reading tags.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// The file could not be opened or read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The container or codec is not recognised or not enabled.
    #[error("unsupported format: {0}")]
    Unsupported(String),
    /// The stream is corrupt and nothing usable could be decoded.
    #[error("corrupt audio: {0}")]
    Corrupt(String),
    /// The file has no audio track or decodes to zero frames.
    #[error("no audio data")]
    Empty,
    /// The cancel flag was set; partial output is discarded.
    #[error("cancelled")]
    Cancelled,
    /// The resampler rejected the rate pair or failed.
    #[error("resample failed: {0}")]
    Resample(String),
    /// Tag parsing failed.
    #[error("cannot read tags: {0}")]
    Tags(String),
}
