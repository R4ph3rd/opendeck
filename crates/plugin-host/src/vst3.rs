//! VST3 backend (cargo feature `vst3`), through the `vst3` bindings
//! (MIT OR Apache-2.0, generated from the MIT-licensed VST 3 SDK headers,
//! pre-generated: no SDK checkout is needed to build).
//!
//! # Scanning
//!
//! [`Vst3Backend::scan_file`] loads a `.vst3` bundle (`module` submodule),
//! calls `GetPluginFactory` and lists the classes of category
//! `Audio Module Class`. Names, vendor, version and sub-categories come from
//! `IPluginFactory3` (unicode), `IPluginFactory2` or `IPluginFactory`, in
//! that order. A class whose sub-categories contain `Instrument` is an
//! instrument, any other audio class an effect. Plugin ids are the class id
//! as 32 uppercase hex digits (keys `vst3:<id>`, the same on every OS). The
//! scanner then validates each class by instantiating it and processing
//! test blocks (`scan::scan_in_process`).
//!
//! # Instances
//!
//! One [`Vst3Instance`] (main thread, not `Send`) per plugin: the component,
//! its `IAudioProcessor` and its `IEditController`, as one object
//! (single-component plugins) or two (separate controller, connected through
//! `IConnectionPoint`). [`PluginInstance::activate`] negotiates stereo main
//! buses, sets up processing at the engine rate and maximum block, and calls
//! `setActive(true)` and `setProcessing(true)` on the main thread; it
//! returns a [`Vst3Processor`] for the audio thread.
//!
//! - Buses: only the main audio input and output (and an instrument's first
//!   event input) are active. A plugin without a stereo main bus is
//!   rejected (mono and surround plugins are post-MVP, ADR 0008).
//! - Parameters: normalized `[0, 1]` as in VST3; see `params` for the
//!   descriptor model. Plain values and value text come from the controller
//!   through [`Vst3Instance::normalized_to_plain`] and
//!   [`Vst3Instance::value_text`].
//! - Host to plugin changes go through a fixed-capacity `IParameterChanges`
//!   with the frame offset kept; edits the user makes in the plugin's own
//!   editor arrive through `IComponentHandler::performEdit` on a ring the
//!   processor drains each block. Values the plugin reports (output
//!   parameter changes) go to the `ParamSink`.
//! - The controller is kept in step: every applied or reported value is
//!   mirrored to the main thread and given to `setParamNormalized` at the
//!   next `service`, `save_state` or `deactivate`.
//! - Latency (`getLatencySamples`) and tail (`getTailSamples`) are read
//!   after activation. VST3 has one value for "no tail" and "does not say";
//!   a zero tail is reported as unknown so the slot keeps processing until
//!   silence.
//! - State: component state then controller state, each as a length-prefixed
//!   block behind the magic `OPV3` (see `params::encode_state`).
//!   `load_state` calls `IComponent::setState`, then
//!   `IEditController::setComponentState` and `setState`.
//! - `IComponentHandler::restartComponent` sets [`InstanceRequests`]:
//!   `kLatencyChanged` -> `latency_changed`, `kReloadComponent`/`kIoChanged`
//!   -> `restart`, parameter flags -> `params_changed`; `performEdit` sets
//!   `state_dirty`.
//! - Host context: `IHostApplication` (name, `IMessage`, `IAttributeList`).
//!   Not implemented yet: `IPlugInterfaceSupport`, `IComponentHandler2/3`,
//!   `IUnitHandler`, `IContextMenu` and editors (`IPlugView`, wave 9a-3).
//!
//! # Threads
//!
//! Loading, creation, activation, state, parameter queries and drop run on
//! the plugin main thread. Only [`Vst3Processor::process`], `reset` and the
//! adapters' parameter queueing run on the audio thread; they allocate
//! nothing. `reset` calls `setProcessing(false)` then `(true)` (VST3 has no
//! reset call); `TODO(verify)` that plugins accept this on the audio thread.

// Loading a VST3 module and calling its COM interfaces is FFI (ADR 0008).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};

use opendeck_contracts::fx::EffectKey;
use opendeck_contracts::plugin::{PluginInfo, PluginKind};

use crate::PluginError;
use crate::backend::{HostWake, PluginBackend, PluginInstance};
use crate::scan::{TargetOs, vst3_folders};

mod events;
mod handler;
mod host_app;
mod instance;
mod module;
mod param_changes;
mod params;
mod processor;
mod stream;
mod util;

pub use handler::EditEvent;
pub use instance::Vst3Instance;
pub use processor::Vst3Processor;

/// Provider id of VST3 plugins.
pub const VST3_PROVIDER: &str = "vst3";

/// The VST3 format.
#[derive(Debug, Default, Clone, Copy)]
pub struct Vst3Backend;

/// Effect or instrument, from the factory's sub-categories (`Fx|Reverb`,
/// `Instrument|Synth`). `Instrument` wins when both appear.
fn kind_of(subcategories: &[String]) -> PluginKind {
    if subcategories.iter().any(|c| c == "Instrument") {
        PluginKind::Instrument
    } else {
        PluginKind::Effect
    }
}

impl PluginBackend for Vst3Backend {
    fn provider(&self) -> &'static str {
        VST3_PROVIDER
    }

    fn extension(&self) -> &'static str {
        "vst3"
    }

    fn default_folders(&self) -> Vec<PathBuf> {
        vst3_folders(TargetOs::current(), |k| std::env::var_os(k))
    }

    fn scan_file(&self, path: &Path) -> Result<Vec<PluginInfo>, PluginError> {
        let module = module::Module::load(path)?;
        let classes = module.classes()?;
        Ok(classes
            .into_iter()
            .map(|c| {
                let id = util::tuid_to_hex(&c.cid);
                PluginInfo {
                    key: EffectKey {
                        provider: VST3_PROVIDER.into(),
                        id: id.clone(),
                    },
                    kind: kind_of(&c.subcategories),
                    name: if c.name.is_empty() { id } else { c.name },
                    vendor: c.vendor,
                    version: c.version,
                    path: module.path().display().to_string(),
                    categories: c.subcategories,
                    // Known only after creating the controller view (wave 9a-3).
                    has_editor: false,
                }
            })
            .collect())
    }

    fn instantiate(
        &self,
        info: &PluginInfo,
        wake: HostWake,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        if info.key.provider != VST3_PROVIDER {
            return Err(PluginError::FormatUnavailable(info.key.provider.clone()));
        }
        Ok(Box::new(Vst3Instance::create(info, wake)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruments_are_told_apart_by_sub_category() {
        let cats = |s: &str| s.split('|').map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(kind_of(&cats("Fx|Reverb")), PluginKind::Effect);
        assert_eq!(kind_of(&cats("Instrument|Synth")), PluginKind::Instrument);
        assert_eq!(kind_of(&cats("Fx|Instrument")), PluginKind::Instrument);
        assert_eq!(kind_of(&[]), PluginKind::Effect);
    }

    #[test]
    fn missing_bundles_fail_to_load() {
        let err = Vst3Backend
            .scan_file(Path::new("/nonexistent/Nothing.vst3"))
            .unwrap_err();
        assert!(matches!(err, PluginError::Load { .. }), "{err:?}");
    }
}
