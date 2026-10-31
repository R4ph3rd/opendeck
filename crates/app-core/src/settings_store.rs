//! Settings persisted in the library's SQLite `settings` table (ADR 0005).
//! A store without a library falls back to `settings.json` in the app config
//! dir. The JSON file is read once to migrate older installs: the first load
//! with a library copies it into the table, which wins from then on.

use std::path::{Path, PathBuf};

use opendeck_contracts::settings::AudioSettings;
use opendeck_library::Library;
use serde::{Deserialize, Serialize};

use crate::types::CoreError;

/// File name inside the config dir.
pub const SETTINGS_FILE: &str = "settings.json";

/// Everything persisted today.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredSettings {
    /// Audio output settings.
    pub audio: AudioSettings,
}

/// Key of [`StoredSettings`] in the library's settings table.
pub const SETTINGS_KEY: &str = "app.settings";

/// Reads and writes [`StoredSettings`]; a store without a path or library
/// keeps nothing.
#[derive(Debug, Clone, Default)]
pub struct SettingsStore {
    path: Option<PathBuf>,
    library: Option<Library>,
}

impl SettingsStore {
    /// Store backed by `dir/settings.json`.
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            path: Some(dir.join(SETTINGS_FILE)),
            library: None,
        }
    }

    /// Keeps settings in `library`'s settings table. A `settings.json` from
    /// an older install (see [`in_dir`](Self::in_dir)) is migrated on the
    /// first load.
    #[must_use]
    pub fn with_library(mut self, library: Library) -> Self {
        self.library = Some(library);
        self
    }

    /// Store that persists nothing (tests, first run without a config dir).
    pub fn memory() -> Self {
        Self::default()
    }

    /// Loads settings. A missing or unreadable file yields defaults, so a
    /// corrupt file never blocks startup.
    pub fn load(&self) -> StoredSettings {
        if let Some(lib) = &self.library {
            match lib.get_setting_json::<StoredSettings>(SETTINGS_KEY) {
                Ok(Some(stored)) => return stored,
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(%err, "stored settings are unreadable, using defaults");
                    return StoredSettings::default();
                }
            }
            // Nothing in the table yet: migrate the JSON file of an older install.
            let migrated = self.load_file();
            if self.path.as_ref().is_some_and(|p| p.is_file())
                && let Err(err) = lib.set_setting_json(SETTINGS_KEY, &migrated)
            {
                tracing::warn!(%err, "could not migrate settings.json");
            }
            return migrated;
        }
        self.load_file()
    }

    fn load_file(&self) -> StoredSettings {
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
        if let Some(lib) = &self.library {
            return lib
                .set_setting_json(SETTINGS_KEY, settings)
                .map_err(|e| CoreError::Settings(e.to_string()));
        }
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
    fn settings_saved_before_m8_load_with_defaults_for_the_new_fields() {
        let d = dir("legacy");
        std::fs::create_dir_all(&d).unwrap();
        let old = r#"{"audio":{"host":null,"device":"DDJ-FLX4","sample_rate":48000,
            "buffer_frames":256,"master":{"left":0,"right":1},
            "headphones":{"left":2,"right":3}}}"#;
        std::fs::write(d.join(SETTINGS_FILE), old).unwrap();
        let loaded = SettingsStore::in_dir(&d).load();
        assert_eq!(loaded.audio.device.as_deref(), Some("DDJ-FLX4"));
        assert_eq!(loaded.audio.buffer_frames, Some(256));
        assert_eq!(loaded.audio.booth, None);
        assert_eq!(loaded.audio.booth_level, 1.0);
        assert!(!loaded.audio.wants_input());
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
    fn json_file_migrates_into_the_library_once() {
        let d = dir("migrate");
        let json_store = SettingsStore::in_dir(&d);
        let mut s = StoredSettings::default();
        s.audio.device = Some("Old Device".into());
        json_store.save(&s).expect("save json");

        let lib = Library::open_in_memory().expect("library");
        let store = SettingsStore::in_dir(&d).with_library(lib.clone());
        assert_eq!(store.load(), s);
        assert!(lib.get_setting(SETTINGS_KEY).unwrap().is_some());

        // The table wins from now on, even if the file changes.
        let mut other = s.clone();
        other.audio.device = Some("Edited File".into());
        json_store.save(&other).expect("save json");
        assert_eq!(store.load(), s);
        let mut next = s.clone();
        next.audio.buffer_frames = Some(128);
        store.save(&next).expect("save db");
        assert_eq!(store.load(), next);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn library_store_without_a_file_uses_defaults() {
        let lib = Library::open_in_memory().expect("library");
        let store = SettingsStore::memory().with_library(lib.clone());
        assert_eq!(store.load(), StoredSettings::default());
        assert!(lib.get_setting(SETTINGS_KEY).unwrap().is_none());
    }

    #[test]
    fn memory_store_keeps_nothing() {
        let store = SettingsStore::memory();
        store.save(&StoredSettings::default()).expect("no-op");
    }
}
