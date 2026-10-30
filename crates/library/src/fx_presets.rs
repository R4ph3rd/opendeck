//! User effect presets (`fx_presets` table, schema 0001).
//!
//! The library keeps no effect types: a preset is a provider, an effect id, a
//! name and the parameter list as JSON text. app-core converts to and from
//! `contracts::fx::FxPreset`.

use rusqlite::params;

use crate::error::{LibraryError, Result};
use crate::library::{Library, now_ms};

/// One stored effect preset.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredFxPreset {
    /// Effect provider (`"builtin"`).
    pub provider: String,
    /// Effect id inside the provider.
    pub effect_id: String,
    /// Preset name, unique per effect.
    pub name: String,
    /// JSON `[[param_id, normalized], ...]`.
    pub params: String,
    /// Wet/dry mix `[0, 1]`.
    pub wet: f64,
    /// Beat division, beats, above zero.
    pub beat_division: f64,
}

impl Library {
    /// Every stored user preset, ordered by effect then name.
    pub fn list_fx_presets(&self) -> Result<Vec<StoredFxPreset>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT provider, effect_id, name, params, wet, beat_division FROM fx_presets \
                 WHERE is_factory = 0 ORDER BY provider, effect_id, name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(StoredFxPreset {
                    provider: r.get(0)?,
                    effect_id: r.get(1)?,
                    name: r.get(2)?,
                    params: r.get(3)?,
                    wet: r.get(4)?,
                    beat_division: r.get(5)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
    }

    /// Inserts a preset or replaces the one with the same provider, effect and name.
    pub fn save_fx_preset(&self, preset: &StoredFxPreset) -> Result<()> {
        if preset.name.trim().is_empty() {
            return Err(LibraryError::Invalid("preset name is empty".into()));
        }
        if !(0.0..=1.0).contains(&preset.wet)
            || preset.beat_division.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
        {
            return Err(LibraryError::Invalid(
                "wet must be 0 to 1 and the beat division above zero".into(),
            ));
        }
        let now = now_ms();
        self.write(|c| {
            c.execute(
                "INSERT INTO fx_presets \
                 (provider, effect_id, name, params, wet, beat_division, is_factory, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7) \
                 ON CONFLICT(provider, effect_id, name) DO UPDATE SET \
                 params = excluded.params, wet = excluded.wet, \
                 beat_division = excluded.beat_division, updated_at = excluded.updated_at",
                params![
                    preset.provider,
                    preset.effect_id,
                    preset.name,
                    preset.params,
                    preset.wet,
                    preset.beat_division,
                    now
                ],
            )?;
            Ok(())
        })
    }

    /// Deletes a preset; returns whether it existed.
    pub fn delete_fx_preset(&self, provider: &str, effect_id: &str, name: &str) -> Result<bool> {
        self.write(|c| {
            Ok(c.execute(
                "DELETE FROM fx_presets WHERE provider = ?1 AND effect_id = ?2 AND name = ?3 \
                 AND is_factory = 0",
                params![provider, effect_id, name],
            )? > 0)
        })
    }
}
