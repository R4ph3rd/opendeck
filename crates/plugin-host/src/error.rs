//! Errors of the plugin host. Control and main thread only.

use std::time::Duration;

use opendeck_contracts::fx::EffectError;

/// Why a plugin operation failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PluginError {
    /// No backend with this provider id is compiled in (cargo features
    /// `clap`, `vst3`).
    #[error("plugin format {0} is not available in this build")]
    FormatUnavailable(String),
    /// The catalog has no plugin with this key, or it has the other kind.
    #[error("unknown plugin {provider}:{id}")]
    UnknownPlugin {
        /// Provider id.
        provider: String,
        /// Plugin id.
        id: String,
    },
    /// The instance was destroyed (its effect or instrument was dropped).
    #[error("plugin instance {0} no longer exists")]
    UnknownInstance(u64),
    /// The library or bundle could not be loaded.
    #[error("cannot load {path}: {reason}")]
    Load {
        /// File or bundle.
        path: String,
        /// Loader message.
        reason: String,
    },
    /// The plugin returned an error from a call.
    #[error("plugin call failed: {0}")]
    Plugin(String),
    /// The main thread did not run the job in time. The job may still run
    /// later; its result is then dropped on the main thread.
    #[error("main thread did not answer within {0:?}")]
    MainThreadTimeout(Duration),
    /// The main-thread executor has shut down.
    #[error("main thread executor is gone")]
    MainThreadGone,
    /// A host thread or process could not be started, or the scanner output
    /// was not understood.
    #[error("scanner: {0}")]
    Scanner(String),
    /// Planned for the named implementation wave (`CONTEXT/plugin-host.md`).
    #[error("not implemented before wave {0}")]
    NotYetImplemented(&'static str),
}

impl From<PluginError> for EffectError {
    fn from(e: PluginError) -> Self {
        match e {
            PluginError::UnknownPlugin { provider, id } => {
                EffectError::UnknownEffect(opendeck_contracts::fx::EffectKey { provider, id })
            }
            PluginError::FormatUnavailable(p) => EffectError::UnknownProvider(p),
            other => EffectError::Failed(other.to_string()),
        }
    }
}
