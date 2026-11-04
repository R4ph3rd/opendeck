//! CLAP backend (cargo feature `clap`), through `clack-host` and
//! `clack-extensions` (MIT OR Apache-2.0).
//!
//! # Scanning
//!
//! `scan_file` lists the plugins of a `.clap` file from its plugin factory.
//! Kind: a descriptor with the `instrument` feature is an instrument; one
//! with `audio-effect` or `analyzer` is an effect; others (note effects
//! only) are not listed. The scanner then validates each plugin by
//! instantiating it (`instantiate` checks the audio ports) and processing
//! test blocks (`scan::scan_in_process`).
//!
//! # Instances
//!
//! One [`ClapInstance`] (main thread, not `Send`) per plugin, holding
//! `clack_host::plugin::PluginInstance`. [`PluginInstance::activate`]
//! activates it at the engine rate (`min_frames` 1, `max_frames` the block
//! size), calls `start_processing` and returns a [`ClapProcessor`] wrapping
//! the started audio processor. Extensions used, all optional except the
//! first:
//!
//! - `audio-ports`: the main input and output port (flag `IS_MAIN`, else
//!   port 0) must have two channels, else the plugin is rejected (mono
//!   plugins are post-MVP, ADR 0008). Other ports get silent input and
//!   discarded output.
//! - `params`: descriptors, and output events for parameter values. Plain
//!   values are converted with the descriptor ([`ParamDescriptor`]); hidden
//!   and read-only parameters are not listed and not settable, but their
//!   output events still reach the UI.
//! - `latency` and `tail`: read at activation.
//! - `state`: `save_state` and `load_state`.
//!
//! Host callbacks (`request_restart`, `request_callback`, `latency.changed`,
//! `params.rescan`, `state.mark_dirty`) only set flags and wake the main
//! thread; [`PluginInstance::service`] runs `on_main_thread` and reports the
//! flags. Host extensions not implemented: `log`, `thread-check`, `gui`
//! (wave 9a-3).
//!
//! # Processing
//!
//! [`ClapProcessor::process`] allocates nothing: audio port tables, input
//! copies, aux buffers and the event lists are sized at activation. Parameter
//! changes become `CLAP_EVENT_PARAM_VALUE` events at their frame offset, so a
//! plugin that honours event times applies them sample-accurately inside the
//! block. A plugin error passes the input through (effects) or outputs
//! silence (instruments) for that block.

// Loading a CLAP entry is FFI; clack-host marks it `unsafe` (ADR 0008).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};

mod host;
mod instance;
mod params;
mod processor;

use clack_host::prelude::PluginEntry;
use opendeck_contracts::fx::EffectKey;

pub use instance::ClapInstance;
use opendeck_contracts::plugin::{PluginInfo, PluginKind};
pub use processor::ClapProcessor;

use crate::PluginError;
use crate::backend::{HostWake, PluginBackend, PluginInstance};
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

    fn instantiate(
        &self,
        info: &PluginInfo,
        wake: HostWake,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        Ok(Box::new(instance::ClapInstance::create(info, wake)?))
    }
}
