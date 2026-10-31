//! [`PluginHost`]: the catalog of scanned plugins, the effect and instrument
//! factories built on it, and handles to live instances.
//!
//! app-core owns one host. It loads the scan records (`scan` module), puts
//! the usable plugins in the catalog with [`PluginHost::set_catalog`] and
//! registers [`PluginHost::effect_factories`] in its `EffectRegistry` and
//! [`PluginHost::instrument_factories`] in its `InstrumentRegistry`. When it
//! needs the main-thread side of an instance (state for presets, editor),
//! it creates through [`PluginHost::create_effect`] or
//! [`PluginHost::create_instrument`], which also return an
//! [`InstanceHandle`].
//!
//! Every method here runs on the control thread and may block up to the
//! host timeout waiting for the main thread.
//!
//! Known gap (wave 9a-1): if instantiation times out and the job runs
//! later, the instance stays in the main-thread table until exit.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use opendeck_contracts::fx::{
    Effect, EffectError, EffectFactory, EffectInfo, EffectKey, FxRouting, ParamDescriptor,
};
use opendeck_contracts::instrument::{Instrument, InstrumentFactory, InstrumentInfo};
use opendeck_contracts::plugin::{PluginInfo, PluginKind};

use crate::PluginError;
use crate::adapter::{AudioHalf, PluginEffect, PluginInstrument};
use crate::backend::{PARAM_RING, ParamChange, PluginBackend};
use crate::instances::{self, Entry, InstanceId};
use crate::main_thread::{DEFAULT_TIMEOUT, MainThread, run_blocking};

/// State of an instance as the main thread last saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStatus {
    /// Created, not activated yet (before `prepare`).
    Inactive,
    /// Activated; its processor belongs to the effect or instrument.
    Active,
    /// Activation failed or timed out; the adapter passes through (effect)
    /// or is silent (instrument).
    Failed(String),
}

struct Shared {
    main: Arc<dyn MainThread>,
    backends: Vec<Arc<dyn PluginBackend>>,
    catalog: RwLock<Vec<PluginInfo>>,
    timeout: Duration,
}

/// The plugin host. Cheap to clone; clones share the catalog.
#[derive(Clone)]
pub struct PluginHost {
    shared: Arc<Shared>,
}

impl PluginHost {
    /// A host with the given backends and main thread, an empty catalog
    /// and [`DEFAULT_TIMEOUT`].
    pub fn new(main: Arc<dyn MainThread>, backends: Vec<Arc<dyn PluginBackend>>) -> Self {
        Self::with_timeout(main, backends, DEFAULT_TIMEOUT)
    }

    /// Same with a custom main-thread timeout.
    pub fn with_timeout(
        main: Arc<dyn MainThread>,
        backends: Vec<Arc<dyn PluginBackend>>,
        timeout: Duration,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                main,
                backends,
                catalog: RwLock::new(Vec::new()),
                timeout,
            }),
        }
    }

    /// Replaces the catalog. Plugins whose provider has no backend are kept
    /// (shown in the browser) but cannot be created.
    pub fn set_catalog(&self, plugins: Vec<PluginInfo>) {
        if let Ok(mut c) = self.shared.catalog.write() {
            *c = plugins;
        }
    }

    /// The current catalog.
    pub fn catalog(&self) -> Vec<PluginInfo> {
        self.shared
            .catalog
            .read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// One effect factory per backend. Factories read the catalog at call
    /// time, so a rescan needs no re-registration.
    pub fn effect_factories(&self) -> Vec<Box<dyn EffectFactory>> {
        self.shared
            .backends
            .iter()
            .map(|b| {
                Box::new(Factory {
                    host: self.clone(),
                    provider: b.provider(),
                }) as Box<dyn EffectFactory>
            })
            .collect()
    }

    /// One instrument factory per backend.
    pub fn instrument_factories(&self) -> Vec<Box<dyn InstrumentFactory>> {
        self.shared
            .backends
            .iter()
            .map(|b| {
                Box::new(Factory {
                    host: self.clone(),
                    provider: b.provider(),
                }) as Box<dyn InstrumentFactory>
            })
            .collect()
    }

    /// Creates an unprepared plugin effect and a handle to its instance.
    pub fn create_effect(
        &self,
        key: &EffectKey,
    ) -> Result<(Box<dyn Effect>, InstanceHandle), PluginError> {
        let (info, half, handle) = self.instantiate(key, PluginKind::Effect)?;
        let info = effect_info(&info, half.1);
        Ok((Box::new(PluginEffect { info, half: half.0 }), handle))
    }

    /// Creates an unprepared plugin instrument and a handle to its instance.
    pub fn create_instrument(
        &self,
        key: &EffectKey,
    ) -> Result<(Box<dyn Instrument>, InstanceHandle), PluginError> {
        let (info, half, handle) = self.instantiate(key, PluginKind::Instrument)?;
        let info = InstrumentInfo {
            key: info.key.clone(),
            name: info.name.clone(),
            params: half.1,
        };
        Ok((Box::new(PluginInstrument { info, half: half.0 }), handle))
    }

    fn find(&self, key: &EffectKey, kind: PluginKind) -> Result<PluginInfo, PluginError> {
        self.catalog()
            .into_iter()
            .find(|p| p.key == *key && p.kind == kind)
            .ok_or_else(|| PluginError::UnknownPlugin {
                provider: key.provider.clone(),
                id: key.id.clone(),
            })
    }

    #[allow(clippy::type_complexity)]
    fn instantiate(
        &self,
        key: &EffectKey,
        kind: PluginKind,
    ) -> Result<
        (
            PluginInfo,
            (AudioHalf, Vec<ParamDescriptor>),
            InstanceHandle,
        ),
        PluginError,
    > {
        let info = self.find(key, kind)?;
        let backend = self
            .shared
            .backends
            .iter()
            .find(|b| b.provider() == key.provider)
            .cloned()
            .ok_or_else(|| PluginError::FormatUnavailable(key.provider.clone()))?;
        let (to_audio, from_main) = rtrb::RingBuffer::<ParamChange>::new(PARAM_RING);
        let (to_main, from_audio) = rtrb::RingBuffer::<ParamChange>::new(PARAM_RING);
        let job_info = info.clone();
        let (id, params) =
            run_blocking(self.shared.main.as_ref(), self.shared.timeout, move || {
                let instance = backend.instantiate(&job_info)?;
                let params: Vec<_> = instance
                    .params()
                    .into_iter()
                    .filter(ParamDescriptor::is_valid)
                    .collect();
                let id = instances::insert(Entry {
                    instance,
                    to_audio,
                    from_audio,
                    status: InstanceStatus::Inactive,
                });
                Ok::<_, PluginError>((id, params))
            })??;
        let half = AudioHalf::new(
            id,
            self.shared.main.clone(),
            self.shared.timeout,
            from_main,
            to_main,
        );
        let handle = InstanceHandle {
            id,
            main: self.shared.main.clone(),
            timeout: self.shared.timeout,
        };
        Ok((info, (half, params), handle))
    }
}

/// `EffectInfo` of a plugin effect. Routing is insert (a plugin may still
/// sit in a send slot); `has_tail` is true because plugins do not reliably
/// report tails, so the slot host keeps processing until -90 dBFS.
fn effect_info(info: &PluginInfo, params: Vec<ParamDescriptor>) -> EffectInfo {
    EffectInfo {
        key: info.key.clone(),
        name: info.name.clone(),
        routing: FxRouting::Insert,
        beat_synced: false,
        has_tail: true,
        params,
    }
}

/// Effect and instrument factory of one provider.
struct Factory {
    host: PluginHost,
    provider: &'static str,
}

impl Factory {
    fn listed(&self, kind: PluginKind) -> impl Iterator<Item = PluginInfo> {
        let provider = self.provider;
        self.host
            .catalog()
            .into_iter()
            .filter(move |p| p.key.provider == provider && p.kind == kind)
    }

    fn key(&self, id: &str) -> EffectKey {
        EffectKey {
            provider: self.provider.to_owned(),
            id: id.to_owned(),
        }
    }
}

impl EffectFactory for Factory {
    fn provider(&self) -> &str {
        self.provider
    }

    /// Lists catalog effects without instantiating them: `params` is empty
    /// here and filled by `create`.
    fn effects(&self) -> Vec<EffectInfo> {
        self.listed(PluginKind::Effect)
            .map(|p| effect_info(&p, Vec::new()))
            .collect()
    }

    fn create(&self, id: &str) -> Result<Box<dyn Effect>, EffectError> {
        Ok(self.host.create_effect(&self.key(id))?.0)
    }
}

impl InstrumentFactory for Factory {
    fn provider(&self) -> &str {
        self.provider
    }

    /// Lists catalog instruments; `params` is filled by `create`.
    fn instruments(&self) -> Vec<InstrumentInfo> {
        self.listed(PluginKind::Instrument)
            .map(|p| InstrumentInfo {
                key: p.key,
                name: p.name,
                params: Vec::new(),
            })
            .collect()
    }

    fn create(&self, id: &str) -> Result<Box<dyn Instrument>, EffectError> {
        Ok(self.host.create_instrument(&self.key(id))?.0)
    }
}

/// Main-thread side of one live instance. Does not keep it alive: once its
/// effect or instrument is dropped, calls fail with
/// [`PluginError::UnknownInstance`]. Control thread; every call may block up
/// to the host timeout.
#[derive(Clone)]
pub struct InstanceHandle {
    id: InstanceId,
    main: Arc<dyn MainThread>,
    timeout: Duration,
}

impl InstanceHandle {
    /// Process-unique id.
    pub fn id(&self) -> InstanceId {
        self.id
    }

    fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Entry) -> Result<T, PluginError> + Send + 'static,
    ) -> Result<T, PluginError> {
        let id = self.id;
        run_blocking(self.main.as_ref(), self.timeout, move || {
            instances::with(id, f)?
        })?
    }

    /// Activation state.
    pub fn status(&self) -> Result<InstanceStatus, PluginError> {
        self.call(|e| Ok(e.status.clone()))
    }

    /// Opaque plugin state for presets and saved sets.
    pub fn save_state(&self) -> Result<Vec<u8>, PluginError> {
        self.call(|e| e.instance.save_state())
    }

    /// Restores state saved by [`InstanceHandle::save_state`].
    pub fn load_state(&self, state: Vec<u8>) -> Result<(), PluginError> {
        self.call(move |e| e.instance.load_state(&state))
    }

    /// Sends a parameter change to the audio half (editor edits, MIDI learn
    /// on the main thread). Dropped if the ring is full.
    pub fn send_param(&self, change: ParamChange) -> Result<(), PluginError> {
        self.call(move |e| {
            let _ = e.to_audio.push(change);
            Ok(())
        })
    }

    /// Takes the parameter changes the plugin reported since the last call
    /// (UI feedback, preset dirty flag).
    pub fn drain_param_output(&self) -> Result<Vec<ParamChange>, PluginError> {
        self.call(|e| {
            let mut out = Vec::new();
            while let Ok(c) = e.from_audio.pop() {
                out.push(c);
            }
            Ok(out)
        })
    }
}
