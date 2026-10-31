//! UI preferences kept in the library's `settings` table under `ui.<name>`.
//!
//! The values are opaque JSON text owned by the UI (first-run state, theme,
//! shortcut map, layout presets). An index key lists the names so a reset
//! can remove them without touching audio or MIDI settings.

use opendeck_library::Library;

use crate::types::CoreError;

const PREFIX: &str = "ui.";
const INDEX_KEY: &str = "ui.__index";

fn valid_name(name: &str) -> Result<(), CoreError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name != "__index"
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!("bad preference name: {name}")))
    }
}

fn lib_err(err: impl std::fmt::Display) -> CoreError {
    CoreError::Library(err.to_string())
}

fn index(library: &Library) -> Result<Vec<String>, CoreError> {
    Ok(library
        .get_setting_json::<Vec<String>>(INDEX_KEY)
        .map_err(lib_err)?
        .unwrap_or_default())
}

/// Reads a preference as JSON text.
pub fn get_pref(library: &Library, name: &str) -> Result<Option<String>, CoreError> {
    valid_name(name)?;
    library
        .get_setting(&format!("{PREFIX}{name}"))
        .map_err(lib_err)
}

/// Writes a preference. `json` must parse as JSON.
pub fn set_pref(library: &Library, name: &str, json: &str) -> Result<(), CoreError> {
    valid_name(name)?;
    serde_json::from_str::<serde_json::Value>(json)
        .map_err(|e| CoreError::Invalid(format!("preference is not JSON: {e}")))?;
    library
        .set_setting(&format!("{PREFIX}{name}"), json)
        .map_err(lib_err)?;
    let mut names = index(library)?;
    if !names.iter().any(|n| n == name) {
        names.push(name.to_owned());
        library
            .set_setting_json(INDEX_KEY, &names)
            .map_err(lib_err)?;
    }
    Ok(())
}

/// Removes every UI preference; returns how many existed.
pub fn reset_prefs(library: &Library) -> Result<u32, CoreError> {
    let names = index(library)?;
    let mut removed = 0;
    for name in &names {
        if library
            .delete_setting(&format!("{PREFIX}{name}"))
            .map_err(lib_err)?
        {
            removed += 1;
        }
    }
    library.delete_setting(INDEX_KEY).map_err(lib_err)?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_reset() {
        let lib = Library::open_in_memory().expect("lib");
        assert_eq!(get_pref(&lib, "firstRun").expect("get"), None);
        set_pref(&lib, "firstRun", "true").expect("set");
        set_pref(&lib, "firstRun", "false").expect("set again");
        set_pref(&lib, "theme", "\"light\"").expect("set");
        assert_eq!(
            get_pref(&lib, "firstRun").expect("get").as_deref(),
            Some("false")
        );
        assert_eq!(reset_prefs(&lib).expect("reset"), 2);
        assert_eq!(get_pref(&lib, "theme").expect("get"), None);
    }

    #[test]
    fn rejects_bad_names_and_values() {
        let lib = Library::open_in_memory().expect("lib");
        assert!(set_pref(&lib, "", "1").is_err());
        assert!(set_pref(&lib, "a b", "1").is_err());
        assert!(set_pref(&lib, "__index", "1").is_err());
        assert!(set_pref(&lib, "ok", "{not json").is_err());
    }

    #[test]
    fn does_not_touch_other_settings() {
        let lib = Library::open_in_memory().expect("lib");
        lib.set_setting("audio", "{}").expect("audio");
        set_pref(&lib, "theme", "\"dark\"").expect("set");
        reset_prefs(&lib).expect("reset");
        assert_eq!(
            lib.get_setting("audio").expect("get").as_deref(),
            Some("{}")
        );
    }
}
