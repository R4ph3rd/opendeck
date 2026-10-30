//! Error types of the MIDI layer.

/// A mapping file could not be loaded or failed validation.
#[derive(Debug, thiserror::Error)]
pub enum MappingError {
    /// The file is not valid JSON or does not match the mapping types.
    #[error("mapping JSON: {0}")]
    Parse(#[from] serde_json::Error),
    /// The file could not be read.
    #[error("mapping file {path}: {source}")]
    Io {
        /// Path that failed.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file parses but breaks a rule of ADR 0007.
    #[error("invalid mapping at {at}: {reason}")]
    Invalid {
        /// Where, e.g. `controls[3] (deck1.play)`.
        at: String,
        /// What is wrong.
        reason: String,
    },
}

impl MappingError {
    pub(crate) fn invalid(at: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Invalid {
            at: at.into(),
            reason: reason.into(),
        }
    }
}

/// A MIDI port or connection failed.
#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    /// The MIDI backend could not be initialised.
    #[error("MIDI backend: {0}")]
    Backend(String),
    /// No port matched.
    #[error("MIDI port not found: {0}")]
    PortNotFound(String),
    /// Opening a port failed (device gone, busy).
    #[error("could not open MIDI port {port}: {reason}")]
    Open {
        /// Port name.
        port: String,
        /// Backend message.
        reason: String,
    },
    /// Sending to an output failed.
    #[error("MIDI send failed: {0}")]
    Send(String),
    /// The controller worker is not running.
    #[error("MIDI controller is not running")]
    NotRunning,
}
