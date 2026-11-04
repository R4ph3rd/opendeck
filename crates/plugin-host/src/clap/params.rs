//! CLAP parameter descriptors and the id lookup used on the audio thread.

use clack_extensions::params::{ParamInfoBuffer, ParamInfoFlags, PluginParams};
use clack_host::prelude::*;
use opendeck_contracts::fx::{ParamCurve, ParamDescriptor, ParamId, ParamUnit};

/// One parameter the plugin exposes.
#[derive(Debug, Clone)]
pub(super) struct ParamEntry {
    /// Descriptor with CLAP plain values; `to_plain` / `to_normalized` convert.
    pub desc: ParamDescriptor,
    /// Settable and visible: listed to the UI and accepted by `set_parameter`.
    /// Hidden and read-only parameters are kept for output events only.
    pub settable: bool,
}

/// Reads every parameter with a valid range, sorted by id (no duplicates).
pub(super) fn read_params(
    ext: Option<PluginParams>,
    plugin: &mut PluginMainThreadHandle<'_>,
) -> Vec<ParamEntry> {
    let Some(ext) = ext else {
        return Vec::new();
    };
    let count = ext.count(plugin);
    let mut buffer = ParamInfoBuffer::new();
    let mut out = Vec::new();
    for index in 0..count {
        let Some(info) = ext.get_info(plugin, index, &mut buffer) else {
            continue;
        };
        let (min, max) = (info.min_value, info.max_value);
        if !(min.is_finite() && max.is_finite() && min < max) {
            continue;
        }
        let stepped = info.flags.contains(ParamInfoFlags::IS_STEPPED);
        let span = max - min;
        let curve = if stepped && (1.0..=65_534.0).contains(&span) && span.fract() == 0.0 {
            ParamCurve::Stepped {
                steps: span as u16 + 1,
            }
        } else {
            ParamCurve::Linear
        };
        let default = if info.default_value.is_finite() {
            info.default_value.clamp(min, max)
        } else {
            min
        };
        let name = String::from_utf8_lossy(info.name).trim().to_owned();
        let id = info.id.get();
        let settable = !info
            .flags
            .intersects(ParamInfoFlags::IS_HIDDEN | ParamInfoFlags::IS_READONLY);
        out.push(ParamEntry {
            desc: ParamDescriptor {
                id: ParamId(id),
                name: if name.is_empty() {
                    format!("Param {id}")
                } else {
                    name
                },
                unit: ParamUnit::None,
                min: min as f32,
                max: max as f32,
                default: default as f32,
                curve,
            },
            settable,
        });
    }
    out.sort_by_key(|e| e.desc.id);
    out.dedup_by_key(|e| e.desc.id);
    out
}

/// Finds a parameter by id. RT-safe (binary search on a sorted slice).
pub(super) fn find(entries: &[ParamEntry], id: ParamId) -> Option<&ParamEntry> {
    entries
        .binary_search_by_key(&id, |e| e.desc.id)
        .ok()
        .and_then(|i| entries.get(i))
}
