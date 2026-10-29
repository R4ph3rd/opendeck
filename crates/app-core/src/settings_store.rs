//! Settings persisted as JSON in the app config dir. Replaced by the SQLite
//! `settings` table in milestone 3 (ADR 0005).

use std::path::{Path, PathBuf};

use opendeck_contracts::settings::AudioSettings;
use serde::{Deserialize, Serialize};

use crate::types::CoreError;

/// File name inside the config dir.
pub const SETTINGS_FILE: &str = "settings.json";

/// Everything persisted today.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredSettings {
    /// Audio output settings.
    pub audio: AudioSettings,
}

/// Reads and writes [`StoredSettings`]; a store without a path keeps nothing.
#[derive(Debug, Clone, Default)]
pub struct SettingsStore {
    path: Option<PathBuf>,
}

impl SettingsStore {
    /// Store backed by `dir/settings.json`.
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            path: Some(dir.join(SETTINGS_FILE)),
        }
    }

    /// Store that persists nothing (tests, first run without a config dir).
    pub fn memory() -> Self {
        Self::default()
    }

    /// Loads settings. A missing or unreadable file yields defaults, so a
    /// corrupt file never blocks startup.
    pub fn load(&self) -> StoredSettings {
        let Some(path) = &self.path else {
            return StoredSettings::default();
        };
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                tracing::warn!(?path, %err, "settings file is invalid, using defaults");
                StoredSettings::default()
            }),
            Err(_) => StoredSettings::default(),
        }
    }

    /// Writes settings atomically (temp file, then rename).
    pub fn save(&self, settings: &StoredSettings) -> Result<(), CoreError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let io = |e: std::io::Error| CoreError::Settings(e.to_string());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let json =
            serde_json::to_vec_pretty(settings).map_err(|e| CoreError::Settings(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(io)?;
        std::fs::rename(&tmp, path).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::settings::{AudioHostKind, ChannelPair};

    fn dir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("opendeck-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn round_trips_and_creates_the_dir() {
        let d = dir("roundtrip");
        let store = SettingsStore::in_dir(&d);
        assert_eq!(store.load(), StoredSettings::default());
        let mut s = StoredSettings::default();
        s.audio.host = Some(AudioHostKind::Alsa);
        s.audio.device = Some("Test".into());
        s.audio.buffer_frames = Some(256);
        s.audio.headphones = Some(ChannelPair { left: 4, right: 5 });
        store.save(&s).expect("save");
        assert_eq!(SettingsStore::in_dir(&d).load(), s);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let d = dir("corrupt");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(SETTINGS_FILE), b"{not json").unwrap();
        assert_eq!(SettingsStore::in_dir(&d).load(), StoredSettings::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn memory_store_keeps_nothing() {
        let store = SettingsStore::memory();
        store.save(&StoredSettings::default()).expect("no-op");
    }
}
