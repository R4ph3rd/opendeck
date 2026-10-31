//! OpenDeck plugin host: CLAP and VST3 effects and instruments behind the
//! contracts [`EffectFactory`](opendeck_contracts::fx::EffectFactory) and
//! [`InstrumentFactory`](opendeck_contracts::instrument::InstrumentFactory)
//! (ADR 0008).
//!
//! # Shape
//!
//! - Discovery runs out of process: [`scan::Scanner`] starts the
//!   `opendeck-plugin-scan` binary once per file, so a plugin that crashes or
//!   hangs while loading only kills the scanner and gets blacklisted.
//! - Processing runs in process: a plugin's audio half processes inside the
//!   engine's FX slot (effects) or instrument slot (instruments) on the
//!   audio thread, with no extra latency.
//! - Every other plugin call runs on the plugin main thread
//!   ([`main_thread::MainThread`], the platform UI thread in the app), where
//!   instances live for their whole life.
//! - Formats plug in through [`backend::PluginBackend`]: [`clap`] (feature
//!   `clap`), [`vst3`] (feature `vst3`) and [`fake`] (tests and CI).
//!
//! # Threads
//!
//! | Call | Thread |
//! |---|---|
//! | `PluginHost::*`, factories, `InstanceHandle::*`, `prepare`, drop | control |
//! | `PluginBackend::instantiate`, `PluginInstance::*` | main (UI) |
//! | `Effect::process` / `Instrument::process`, `set_parameter`, `reset` | audio |
//! | `PluginBackend::scan_file` | scanner subprocess |

pub mod adapter;
pub mod backend;
#[cfg(feature = "clap")]
pub mod clap;
mod error;
pub mod fake;
mod host;
mod instances;
pub mod main_thread;
pub mod scan;
#[cfg(feature = "vst3")]
pub mod vst3;

use std::sync::Arc;

pub use adapter::{PluginEffect, PluginInstrument};
pub use backend::PluginBackend;
pub use error::PluginError;
pub use host::{InstanceHandle, InstanceStatus, PluginHost};
pub use instances::InstanceId;

/// The real formats compiled into this build (cargo features `clap`,
/// `vst3`), in browser order. The fake format is never included.
pub fn compiled_backends() -> Vec<Arc<dyn PluginBackend>> {
    let all: [Option<Arc<dyn PluginBackend>>; 2] = [
        #[cfg(feature = "clap")]
        Some(Arc::new(clap::ClapBackend)),
        #[cfg(not(feature = "clap"))]
        None,
        #[cfg(feature = "vst3")]
        Some(Arc::new(vst3::Vst3Backend)),
        #[cfg(not(feature = "vst3"))]
        None,
    ];
    all.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests;
