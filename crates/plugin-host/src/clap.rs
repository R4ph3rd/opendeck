//! CLAP backend (cargo feature `clap`), through `clack-host`
//! (MIT OR Apache-2.0).
//!
//! Wave 9a-1 status: scanning lists the plugins of a `.clap` file from its
//! plugin factory. Instances (`instantiate`) come with the rest of wave 9a-1:
//! `clack_host::plugin::PluginInstance` on the main thread,
//! `StartedPluginAudioProcessor` as the [`PluginProcessor`](crate::backend::PluginProcessor),
//! the `params`, `latency`, `state` and `audio-ports` extensions.
//!
//! Kind: a descriptor with the `instrument` feature is an instrument; one
//! with `audio-effect` or `analyzer` is an effect; others (note effects
//! only) are not listed.

// Loading a CLAP entry is FFI; clack-host marks it `unsafe` (ADR 0008).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};

use clack_host::prelude::PluginEntry;
use opendeck_contracts::fx::EffectKey;
use opendeck_contracts::plugin::{PluginInfo, PluginKind};

use crate::PluginError;
use crate::backend::{PluginBackend, PluginInstance};
use crate::scan::{TargetOs, clap_folders};

/// Provider id of CLAP plugins.
pub const CLAP_PROVIDER: &str = "clap";

/// The CLAP format.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClapBackend;

impl PluginBackend for ClapBackend {
    fn provider(&self) -> &'static str {
        CLAP_PROVIDER
    }

    fn extension(&self) -> &'static str {
        "clap"
    }

    fn default_folders(&self) -> Vec<PathBuf> {
        clap_folders(TargetOs::current(), |k| std::env::var_os(k))
    }

    fn scan_file(&self, path: &Path) -> Result<Vec<PluginInfo>, PluginError> {
        let load_err = |reason: String| PluginError::Load {
            path: path.display().to_string(),
            reason,
        };
        // SAFETY: runs plugin code; only called in the scanner subprocess,
        // where a misbehaving entry can only crash that process.
        let entry = unsafe { PluginEntry::load(path) }.map_err(|e| load_err(e.to_string()))?;
        let factory = entry
            .get_plugin_factory()
            .ok_or_else(|| load_err("no plugin factory".into()))?;
        let text = |s: Option<&std::ffi::CStr>| {
            s.map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let mut out = Vec::new();
        for d in factory.plugin_descriptors() {
            let Some(id) = d.id() else { continue };
            let categories: Vec<String> = d
                .features()
                .map(|f| f.to_string_lossy().into_owned())
                .collect();
            let has = |f: &str| categories.iter().any(|c| c == f);
            let kind = if has("instrument") {
                PluginKind::Instrument
            } else if has("audio-effect") || has("analyzer") {
                PluginKind::Effect
            } else {
                continue;
            };
            let id = id.to_string_lossy().into_owned();
            let name = text(d.name());
            out.push(PluginInfo {
                key: EffectKey {
                    provider: CLAP_PROVIDER.into(),
                    id: id.clone(),
                },
                kind,
                name: if name.is_empty() { id } else { name },
                vendor: text(d.vendor()),
                version: text(d.version()),
                path: path.display().to_string(),
                categories,
                // Known only after instantiation (gui extension), wave 9a-3.
                has_editor: false,
            });
        }
        Ok(out)
    }

    fn instantiate(&self, _: &PluginInfo) -> Result<Box<dyn PluginInstance>, PluginError> {
        Err(PluginError::NotYetImplemented("9a-1"))
    }
}
