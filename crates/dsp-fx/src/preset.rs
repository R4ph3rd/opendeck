//! Presets: factory presets derived from the catalog, JSON helpers for
//! `FxPreset`, and the chain preset model. Persistence (the `fx_presets`
//! table, preset files) lives in library and app-core, not here.

use crate::catalog::{self, CATALOG, EffectSpec};
use opendeck_contracts::fx::{Effect, EffectKey, FxPreset, ParamId};
use opendeck_contracts::ids::FX_SLOTS_PER_UNIT;
use opendeck_contracts::units::Beats;
use serde::{Deserialize, Serialize};

fn key_of(spec: &EffectSpec) -> EffectKey {
    EffectKey {
        provider: crate::PROVIDER_ID.to_owned(),
        id: spec.id.to_owned(),
    }
}

/// Factory presets of one built-in effect, with every parameter set
/// (normalized). Empty for an unknown id.
pub fn factory_presets(effect_id: &str) -> Vec<FxPreset> {
    let Some(spec) = catalog::find(effect_id) else {
        return Vec::new();
    };
    let info = spec.info();
    spec.presets
        .iter()
        .map(|pr| FxPreset {
            effect: key_of(spec),
            name: pr.name.to_owned(),
            params: info
                .params
                .iter()
                .zip(spec.params)
                .map(|(d, ps)| {
                    let plain = pr
                        .params
                        .iter()
                        .find(|(k, _)| *k == ps.key)
                        .map_or(ps.default, |(_, v)| *v);
                    (d.id, d.to_normalized(plain))
                })
                .collect(),
            wet: pr.wet,
            beat_division: Beats(pr.division),
        })
        .collect()
}

/// Factory presets of every built-in effect, in catalog order.
pub fn all_factory_presets() -> Vec<FxPreset> {
    CATALOG.iter().flat_map(|s| factory_presets(s.id)).collect()
}

/// Sets every parameter of `preset` on `effect`. Wet and beat division are
/// host settings and are not applied here. Unknown ids are ignored by the effect.
pub fn apply_preset(effect: &mut dyn Effect, preset: &FxPreset) {
    for (id, value) in &preset.params {
        effect.set_parameter(*id, *value);
    }
}

/// Serialises a preset as JSON.
pub fn preset_to_json(preset: &FxPreset) -> Result<String, serde_json::Error> {
    serde_json::to_string(preset)
}

/// Parses a preset from JSON.
pub fn preset_from_json(json: &str) -> Result<FxPreset, serde_json::Error> {
    serde_json::from_str(json)
}

/// One slot of a chain preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainSlot {
    /// Effect type in this slot.
    pub effect: EffectKey,
    /// Normalized parameters by id.
    pub params: Vec<(ParamId, f32)>,
    /// Wet/dry mix `[0, 1]`.
    pub wet: f32,
    /// Beat division for beat-synced effects.
    pub beat_division: Beats,
    /// Slot enabled (not bypassed).
    pub enabled: bool,
}

impl ChainSlot {
    /// Slot from an effect preset.
    pub fn from_preset(preset: &FxPreset, enabled: bool) -> Self {
        Self {
            effect: preset.effect.clone(),
            params: preset.params.clone(),
            wet: preset.wet,
            beat_division: preset.beat_division,
            enabled,
        }
    }
}

/// Ordered effect slots, applied to one FX unit. Model only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainPreset {
    /// Preset name.
    pub name: String,
    /// Slots in processing order, at most [`FX_SLOTS_PER_UNIT`].
    pub slots: Vec<ChainSlot>,
}

/// Why a chain preset is not usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainError {
    /// More slots than an FX unit has.
    TooManySlots(usize),
    /// A wet value is outside `[0, 1]` or not finite.
    BadWet(usize),
}

impl core::fmt::Display for ChainError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooManySlots(n) => {
                write!(f, "chain has {n} slots, at most {FX_SLOTS_PER_UNIT} fit")
            }
            Self::BadWet(i) => write!(f, "slot {i} has a wet value outside 0..1"),
        }
    }
}

impl std::error::Error for ChainError {}

impl ChainPreset {
    /// Checks the slot count and wet ranges.
    pub fn validate(&self) -> Result<(), ChainError> {
        if self.slots.len() > FX_SLOTS_PER_UNIT {
            return Err(ChainError::TooManySlots(self.slots.len()));
        }
        match self
            .slots
            .iter()
            .position(|s| !(0.0..=1.0).contains(&s.wet))
        {
            Some(i) => Err(ChainError::BadWet(i)),
            None => Ok(()),
        }
    }

    /// Serialises as JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parses from JSON (does not validate).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
