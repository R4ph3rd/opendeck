//! The main-thread half of a CLAP instance.

use std::path::Path;
use std::sync::Arc;

use clack_extensions::audio_ports::{AudioPortFlags, AudioPortInfoBuffer, PluginAudioPorts};
use clack_extensions::latency::PluginLatency;
use clack_extensions::params::PluginParams;
use clack_extensions::state::PluginState;
use clack_extensions::tail::PluginTail;
use clack_host::prelude::*;
use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::ParamDescriptor;
use opendeck_contracts::plugin::{PluginInfo, PluginKind};

use super::host::{AudioH, Flags, Host, MainH, SharedH};
use super::params::{ParamEntry, read_params};
use super::processor::{ClapProcessor, PortSpec};
use crate::PluginError;
use crate::backend::{HostWake, InstanceRequests, PluginInstance, PluginProcessor};

/// Port layout found at instantiation.
struct Ports {
    inputs: Vec<PortSpec>,
    outputs: Vec<PortSpec>,
}

/// One CLAP plugin instance. Lives on the main thread.
pub struct ClapInstance {
    // Field order matters: the instance drops before the entry that holds
    // its library.
    instance: clack_host::plugin::PluginInstance<Host>,
    _entry: PluginEntry,
    flags: Arc<Flags>,
    kind: PluginKind,
    params: Arc<[ParamEntry]>,
    ports: Ports,
    latency_ext: Option<PluginLatency>,
    state_ext: Option<PluginState>,
    tail_ext: Option<PluginTail>,
    latency: u32,
    tail: Option<u32>,
}

impl ClapInstance {
    /// Loads `info.path` and creates the plugin. Main thread.
    pub(super) fn create(info: &PluginInfo, wake: HostWake) -> Result<Self, PluginError> {
        let path = Path::new(&info.path);
        let load_err = |reason: String| PluginError::Load {
            path: info.path.clone(),
            reason,
        };
        // SAFETY: runs plugin code (library init). Only reached for plugins
        // that passed a scan in the scanner subprocess.
        let entry = unsafe { PluginEntry::load(path) }.map_err(|e| load_err(e.to_string()))?;
        let id = std::ffi::CString::new(info.key.id.as_str())
            .map_err(|_| load_err("plugin id contains a NUL byte".into()))?;
        let host_info = HostInfo::new(
            "OpenDeck",
            "OpenDeck",
            "https://github.com/R4ph3rd/opendeck",
            env!("CARGO_PKG_VERSION"),
        )
        .map_err(|e| load_err(e.to_string()))?;
        let flags = Arc::new(Flags::default());
        let (shared_flags, main_flags) = (flags.clone(), flags.clone());
        let (shared_wake, main_wake) = (wake.clone(), wake);
        let mut instance = clack_host::plugin::PluginInstance::<Host>::new(
            move |_| SharedH {
                flags: shared_flags,
                wake: shared_wake,
            },
            move |_| MainH {
                flags: main_flags,
                wake: main_wake,
            },
            &entry,
            &id,
            &host_info,
        )
        .map_err(|e| PluginError::Plugin(format!("create {}: {e}", info.key.id)))?;

        let mut handle = instance.plugin_handle();
        let ports_ext = handle.get_extension::<PluginAudioPorts>();
        let params_ext = handle.get_extension::<PluginParams>();
        let latency_ext = handle.get_extension::<PluginLatency>();
        let state_ext = handle.get_extension::<PluginState>();
        let tail_ext = handle.get_extension::<PluginTail>();
        let ports = read_ports(ports_ext, &handle, info.kind)?;
        let params: Arc<[ParamEntry]> = read_params(params_ext, &mut handle).into();
        Ok(Self {
            instance,
            _entry: entry,
            flags,
            kind: info.kind,
            params,
            ports,
            latency_ext,
            state_ext,
            tail_ext,
            latency: 0,
            tail: None,
        })
    }
}

/// Reads the audio ports and checks the main ports are stereo.
fn read_ports(
    ext: Option<PluginAudioPorts>,
    handle: &PluginMainThreadHandle<'_>,
    kind: PluginKind,
) -> Result<Ports, PluginError> {
    let ext = ext.ok_or_else(|| PluginError::Plugin("no audio-ports extension".into()))?;
    let mut buffer = AudioPortInfoBuffer::new();
    let mut read = |is_input: bool| -> Vec<PortSpec> {
        let count = ext.count(handle, is_input);
        let mut ports: Vec<PortSpec> = Vec::new();
        for index in 0..count {
            if let Some(info) = ext.get(handle, index, is_input, &mut buffer) {
                ports.push(PortSpec {
                    channels: info.channel_count,
                    is_main: info.flags.contains(AudioPortFlags::IS_MAIN),
                });
            }
        }
        // Exactly one main port: the flagged one, else the first.
        let flagged = ports.iter().position(|p| p.is_main).unwrap_or(0);
        for (i, p) in ports.iter_mut().enumerate() {
            p.is_main = i == flagged;
        }
        ports
    };
    let inputs = read(true);
    let outputs = read(false);
    let stereo = |ports: &[PortSpec]| ports.iter().any(|p| p.is_main && p.channels == 2);
    let ok = match kind {
        PluginKind::Effect => stereo(&inputs) && stereo(&outputs),
        PluginKind::Instrument => stereo(&outputs),
    };
    if !ok {
        return Err(PluginError::Plugin(
            "no stereo main audio port (mono and surround plugins are not supported)".into(),
        ));
    }
    Ok(Ports { inputs, outputs })
}

impl PluginInstance for ClapInstance {
    fn params(&self) -> Vec<ParamDescriptor> {
        self.params
            .iter()
            .filter(|e| e.settable)
            .map(|e| e.desc.clone())
            .collect()
    }

    fn activate(
        &mut self,
        sample_rate: SampleRate,
        max_block_frames: usize,
    ) -> Result<Box<dyn PluginProcessor>, PluginError> {
        if sample_rate.0 == 0 || max_block_frames == 0 {
            return Err(PluginError::Plugin(
                "sample rate and block size must be positive".into(),
            ));
        }
        let config = PluginAudioConfiguration {
            sample_rate: sample_rate.as_f64(),
            min_frames_count: 1,
            max_frames_count: u32::try_from(max_block_frames).unwrap_or(u32::MAX),
        };
        let stopped = self
            .instance
            .activate(|_, _| AudioH, config)
            .map_err(|e| PluginError::Plugin(format!("activate: {e}")))?;
        let started = match stopped.start_processing() {
            Ok(s) => s,
            Err(e) => {
                drop(e.into_stopped_processor());
                let _ = self.instance.try_deactivate();
                return Err(PluginError::Plugin("start_processing failed".into()));
            }
        };
        let mut processor = ClapProcessor::new(
            started,
            self.params.clone(),
            &self.ports.inputs,
            &self.ports.outputs,
            max_block_frames,
            self.kind == PluginKind::Effect,
        );
        self.latency = {
            let handle = self.instance.plugin_handle();
            self.latency_ext.map_or(0, |ext| ext.get(&handle))
        };
        self.tail = match (self.tail_ext, processor.plugin_handle()) {
            (Some(ext), Some(handle)) => Some(ext.get(&handle).to_raw()),
            _ => None,
        };
        Ok(Box::new(processor))
    }

    fn deactivate(&mut self, processor: Box<dyn PluginProcessor>) {
        // Dropping the processor stops processing and releases its handle,
        // which lets the instance deactivate.
        drop(processor);
        let _ = self.instance.try_deactivate();
        self.latency = 0;
    }

    fn latency_frames(&self) -> u32 {
        self.latency
    }

    fn tail_frames(&self) -> Option<u32> {
        self.tail
    }

    fn service(&mut self) -> InstanceRequests {
        if self.flags.take_callback() {
            self.instance.call_on_main_thread_callback();
        }
        self.flags.take_requests()
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        let ext = self
            .state_ext
            .ok_or_else(|| PluginError::Plugin("plugin has no state extension".into()))?;
        let handle = self.instance.plugin_handle();
        let mut bytes = Vec::new();
        ext.save(&handle, &mut bytes)
            .map_err(|e| PluginError::Plugin(e.to_string()))?;
        Ok(bytes)
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        let ext = self
            .state_ext
            .ok_or_else(|| PluginError::Plugin("plugin has no state extension".into()))?;
        let handle = self.instance.plugin_handle();
        let mut reader = state;
        ext.load(&handle, &mut reader)
            .map_err(|e| PluginError::Plugin(e.to_string()))
    }
}
