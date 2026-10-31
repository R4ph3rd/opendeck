//! Host side of the CLAP callbacks: flags set by the plugin (from any
//! thread) and read by the main thread in `PluginInstance::service`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clack_extensions::latency::{HostLatency, HostLatencyImpl};
use clack_extensions::params::{
    HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags, ParamRescanFlags,
};
use clack_extensions::state::{HostState, HostStateImpl};
use clack_host::prelude::*;

use crate::backend::{HostWake, InstanceRequests};

/// Flags shared by the plugin's callbacks and the instance.
#[derive(Default)]
pub(super) struct Flags {
    callback: AtomicBool,
    restart: AtomicBool,
    latency: AtomicBool,
    params: AtomicBool,
    dirty: AtomicBool,
}

impl Flags {
    /// True (once) if the plugin asked for `on_main_thread`.
    pub fn take_callback(&self) -> bool {
        self.callback.swap(false, Ordering::AcqRel)
    }

    /// Takes the requests made since the last call.
    pub fn take_requests(&self) -> InstanceRequests {
        InstanceRequests {
            restart: self.restart.swap(false, Ordering::AcqRel),
            latency_changed: self.latency.swap(false, Ordering::AcqRel),
            params_changed: self.params.swap(false, Ordering::AcqRel),
            state_dirty: self.dirty.swap(false, Ordering::AcqRel),
        }
    }
}

/// Handlers valid on every thread.
pub(super) struct SharedH {
    pub flags: Arc<Flags>,
    pub wake: HostWake,
}

impl SharedHandler<'_> for SharedH {
    fn request_restart(&self) {
        self.flags.restart.store(true, Ordering::Release);
        (self.wake)();
    }

    fn request_process(&self) {
        // The slot host processes every block; nothing to do.
    }

    fn request_callback(&self) {
        self.flags.callback.store(true, Ordering::Release);
        (self.wake)();
    }
}

impl HostParamsImplShared for SharedH {
    fn request_flush(&self) {
        // Parameter changes reach the plugin in `process`; the audio thread
        // runs continuously while the instance is active.
    }
}

/// Main-thread handlers.
pub(super) struct MainH {
    pub flags: Arc<Flags>,
    pub wake: HostWake,
}

impl MainThreadHandler<'_> for MainH {}

impl HostLatencyImpl for MainH {
    fn changed(&self) {
        self.flags.latency.store(true, Ordering::Release);
        (self.wake)();
    }
}

impl HostParamsImplMainThread for MainH {
    fn rescan(&self, flags: ParamRescanFlags) {
        self.flags.params.store(true, Ordering::Release);
        if flags.requires_restart() {
            self.flags.restart.store(true, Ordering::Release);
        }
        (self.wake)();
    }

    fn clear(&self, _: ClapId, _: ParamClearFlags) {}
}

impl HostStateImpl for MainH {
    fn mark_dirty(&self) {
        self.flags.dirty.store(true, Ordering::Release);
        (self.wake)();
    }
}

/// Audio-thread handler: no callbacks are used on that side.
pub(super) struct AudioH;

impl AudioProcessorHandler<'_> for AudioH {}

/// The host type of every CLAP instance.
pub(super) struct Host;

impl HostHandlers for Host {
    type Shared<'a> = SharedH;
    type MainThread<'a> = MainH;
    type AudioProcessor<'a> = AudioH;

    fn declare_extensions(builder: &mut HostExtensions<Self>, _: &Self::Shared<'_>) {
        builder
            .register::<HostLatency>()
            .register::<HostParams>()
            .register::<HostState>();
    }
}
