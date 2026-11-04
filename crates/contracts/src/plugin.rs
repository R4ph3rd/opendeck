//! External plugin metadata shared by the plugin host, app-core and the UI
//! (ADR 0008). Control thread only; nothing here crosses the audio thread.
//!
//! A scanned plugin is identified by its [`EffectKey`]: `provider` is the
//! plugin format (`"clap"` or `"vst3"`) and `id` the format's stable plugin
//! id (CLAP: the descriptor id, reverse-DNS; VST3: the class id as 32
//! uppercase hex digits). The file path is not part of the key, so moving a
//! plugin keeps presets and slot assignments.

use crate::fx::EffectKey;

/// What a plugin is used as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PluginKind {
    /// Audio in, audio out: offered by the plugin effect factories.
    Effect,
    /// Notes in, audio out: offered by the plugin instrument factories.
    Instrument,
}

/// One plugin found by the scanner.
///
/// Invariants: `key.provider` is the format id; `name` is not empty;
/// `path` is the absolute path of the file or bundle that holds the plugin
/// (one file can hold several plugins), as text for display.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginInfo {
    /// Format and stable plugin id.
    pub key: EffectKey,
    /// Effect or instrument.
    pub kind: PluginKind,
    /// Display name.
    pub name: String,
    /// Vendor, empty if the plugin gives none.
    pub vendor: String,
    /// Plugin version string as the plugin reports it, empty if none.
    pub version: String,
    /// File or bundle path.
    pub path: String,
    /// Format categories or features as reported (`"audio-effect"`,
    /// `"Fx|Delay"`), for browser filters. Never parsed for behaviour except
    /// to derive `kind`.
    pub categories: Vec<String>,
    /// The plugin offers an editor window.
    pub has_editor: bool,
}

/// Outcome of scanning one plugin file in the scanner subprocess.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "status", rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum PluginScanStatus {
    /// Loaded, instantiated and processed a test block.
    Ok,
    /// The scanner reported an error (not a plugin, wrong architecture,
    /// unsupported format, failed validation). Not retried until the file
    /// changes.
    Failed {
        /// Short reason for the scan settings view.
        reason: String,
    },
    /// The scanner process died. The file is blacklisted until the user
    /// retries it or the file changes.
    Crashed,
    /// The scanner did not finish within the timeout and was killed.
    /// Blacklisted like [`PluginScanStatus::Crashed`].
    TimedOut,
}

impl PluginScanStatus {
    /// True for outcomes that blacklist the file.
    pub fn blacklists(&self) -> bool {
        matches!(self, Self::Crashed | Self::TimedOut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crashes_and_timeouts_blacklist() {
        assert!(PluginScanStatus::Crashed.blacklists());
        assert!(PluginScanStatus::TimedOut.blacklists());
        assert!(!PluginScanStatus::Ok.blacklists());
        assert!(!PluginScanStatus::Failed { reason: "x".into() }.blacklists());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_forms_are_stable() {
        let s = serde_json::to_string(&PluginScanStatus::Failed {
            reason: "no entry".into(),
        })
        .expect("serialize");
        assert_eq!(s, r#"{"status":"failed","reason":"no entry"}"#);
        assert_eq!(
            serde_json::to_string(&PluginKind::Instrument).expect("serialize"),
            r#""instrument""#
        );
    }
}
