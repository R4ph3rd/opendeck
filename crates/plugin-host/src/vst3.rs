//! VST3 backend (cargo feature `vst3`), through the `vst3` bindings
//! (MIT OR Apache-2.0, generated from the MIT-licensed VST 3 SDK headers).
//!
//! Wave 9a-2 status: folders and bundle layout are in place; scanning and
//! instances are not. The wave loads the bundle library
//! ([`vst3_binary_path`](crate::scan::vst3_binary_path)), calls
//! `GetPluginFactory`, lists classes of category `kVstAudioEffectClass`
//! (`IPluginFactory2` sub-categories `Fx` / `Instrument` give the kind), and
//! hosts `IComponent` + `IAudioProcessor` (audio half) and `IEditController`
//! (main thread). Plugin ids are the class id as 32 uppercase hex digits.

use std::path::{Path, PathBuf};

use opendeck_contracts::plugin::PluginInfo;

use crate::PluginError;
use crate::backend::{PluginBackend, PluginInstance};
use crate::scan::{TargetOs, vst3_folders};

/// Provider id of VST3 plugins.
pub const VST3_PROVIDER: &str = "vst3";

/// The factory interface wave 9a-2 starts from (`GetPluginFactory` result).
pub type Vst3Factory = vst3::Steinberg::IPluginFactory;

/// The VST3 format.
#[derive(Debug, Default, Clone, Copy)]
pub struct Vst3Backend;

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

    fn scan_file(&self, _: &Path) -> Result<Vec<PluginInfo>, PluginError> {
        Err(PluginError::NotYetImplemented("9a-2"))
    }

    fn instantiate(&self, _: &PluginInfo) -> Result<Box<dyn PluginInstance>, PluginError> {
        Err(PluginError::NotYetImplemented("9a-2"))
    }
}
