//! `IComponentHandler`: what the controller tells the host.
//!
//! - `performEdit` (the user moves a control in the plugin's editor) goes to
//!   the audio half as a parameter change, through the ring the processor
//!   owns, and is also kept in a bounded log for the UI (preset dirty flag,
//!   editor automation later).
//! - `restartComponent` flags become [`InstanceRequests`].
//!
//! Everything here is called by the plugin on the main thread (the VST3
//! rules), but the state is thread-safe so a plugin that calls from
//! elsewhere does no harm.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use opendeck_contracts::fx::ParamId;
use vst3::Steinberg::Vst::{
    IComponentHandler, IComponentHandlerTrait, ParamID, ParamValue, RestartFlags_,
};
use vst3::Steinberg::{int32, kResultOk, tresult};
use vst3::{Class, ComWrapper};

use super::util::e;
use crate::backend::{HostWake, InstanceRequests, ParamChange};

/// Edits kept for the UI between reads; further edits are dropped.
const EDIT_LOG: usize = 1024;

/// One edit gesture step reported by the plugin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditEvent {
    /// The user started changing a parameter (`beginEdit`).
    Begin(ParamId),
    /// A new normalized value (`performEdit`).
    Value(ParamId, f64),
    /// The gesture ended (`endEdit`).
    End(ParamId),
}

/// State shared between the handler object and the instance.
#[derive(Default)]
pub(super) struct HandlerState {
    restart: AtomicBool,
    latency: AtomicBool,
    params: AtomicBool,
    dirty: AtomicBool,
    log: Mutex<Vec<EditEvent>>,
    /// Producer to the current processor; `None` while inactive.
    to_audio: Mutex<Option<rtrb::Producer<ParamChange>>>,
}

impl HandlerState {
    /// Sets (or clears) the ring to the active processor.
    pub(super) fn set_audio_ring(&self, p: Option<rtrb::Producer<ParamChange>>) {
        if let Ok(mut slot) = self.to_audio.lock() {
            *slot = p;
        }
    }

    /// Takes the requests made since the last call.
    pub(super) fn take_requests(&self) -> InstanceRequests {
        InstanceRequests {
            restart: self.restart.swap(false, Ordering::AcqRel),
            latency_changed: self.latency.swap(false, Ordering::AcqRel),
            params_changed: self.params.swap(false, Ordering::AcqRel),
            state_dirty: self.dirty.swap(false, Ordering::AcqRel),
        }
    }

    /// Takes the edit log.
    pub(super) fn take_edits(&self) -> Vec<EditEvent> {
        self.log
            .lock()
            .map(|mut l| std::mem::take(&mut *l))
            .unwrap_or_default()
    }

    fn record(&self, e: EditEvent) {
        if let Ok(mut log) = self.log.lock()
            && log.len() < EDIT_LOG
        {
            log.push(e);
        }
    }
}

/// The COM object given to `IEditController::setComponentHandler`.
pub(super) struct ComponentHandler {
    state: Arc<HandlerState>,
    wake: HostWake,
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl ComponentHandler {
    pub(super) fn new(state: Arc<HandlerState>, wake: HostWake) -> ComWrapper<ComponentHandler> {
        ComWrapper::new(ComponentHandler { state, wake })
    }

    /// The interface pointer for `setComponentHandler`. Valid while
    /// `wrapper` lives.
    pub(super) fn as_ptr(wrapper: &ComWrapper<ComponentHandler>) -> *mut IComponentHandler {
        wrapper
            .as_com_ref::<IComponentHandler>()
            .map_or(std::ptr::null_mut(), |r| r.as_ptr())
    }
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, id: ParamID) -> tresult {
        self.state.record(EditEvent::Begin(ParamId(id)));
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        if value.is_finite() {
            let v = value.clamp(0.0, 1.0);
            if let Ok(mut ring) = self.state.to_audio.lock()
                && let Some(p) = ring.as_mut()
            {
                // A full ring drops the edit: the audio thread is behind.
                let _ = p.push(ParamChange {
                    id: ParamId(id),
                    normalized: v as f32,
                    offset: 0,
                });
            }
            self.state.record(EditEvent::Value(ParamId(id), v));
            self.state.dirty.store(true, Ordering::Release);
            self.wake.notify();
        }
        kResultOk
    }

    unsafe fn endEdit(&self, id: ParamID) -> tresult {
        self.state.record(EditEvent::End(ParamId(id)));
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        use RestartFlags_::{
            kIoChanged, kLatencyChanged, kParamTitlesChanged, kParamValuesChanged, kReloadComponent,
        };
        if flags & e(kReloadComponent | kIoChanged) != 0 {
            self.state.restart.store(true, Ordering::Release);
        }
        if flags & e(kLatencyChanged) != 0 {
            self.state.latency.store(true, Ordering::Release);
        }
        if flags & e(kParamValuesChanged | kParamTitlesChanged) != 0 {
            self.state.params.store(true, Ordering::Release);
        }
        self.wake.notify();
        kResultOk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handler() -> (Arc<HandlerState>, ComWrapper<ComponentHandler>, HostWake) {
        let state = Arc::new(HandlerState::default());
        let wake: HostWake = Arc::new(crate::backend::WakeFlag::default());
        (
            state.clone(),
            ComponentHandler::new(state, wake.clone()),
            wake,
        )
    }

    #[test]
    fn edits_reach_the_audio_ring_and_the_log() {
        let (state, h, wakes) = handler();
        let (tx, mut rx) = rtrb::RingBuffer::new(8);
        state.set_audio_ring(Some(tx));
        let p = h.as_com_ref::<IComponentHandler>().expect("interface");
        // SAFETY: plain calls on a live object.
        unsafe {
            p.beginEdit(3);
            p.performEdit(3, 0.25);
            p.performEdit(3, f64::NAN); // ignored
            p.performEdit(3, 7.0); // clamped
            p.endEdit(3);
        }
        assert_eq!(
            rx.pop(),
            Ok(ParamChange {
                id: ParamId(3),
                normalized: 0.25,
                offset: 0
            })
        );
        assert_eq!(rx.pop().map(|c| c.normalized), Ok(1.0));
        assert!(rx.pop().is_err());
        assert_eq!(
            state.take_edits(),
            [
                EditEvent::Begin(ParamId(3)),
                EditEvent::Value(ParamId(3), 0.25),
                EditEvent::Value(ParamId(3), 1.0),
                EditEvent::End(ParamId(3)),
            ]
        );
        assert!(state.take_edits().is_empty());
        assert!(state.take_requests().state_dirty);
        assert!(!state.take_requests().state_dirty);
        assert!(wakes.take(), "the host is asked for a service pass");
        assert!(!wakes.take());
    }

    #[test]
    fn restart_flags_become_requests() {
        let (state, h, _) = handler();
        let p = h.as_com_ref::<IComponentHandler>().expect("interface");
        // SAFETY: plain call on a live object.
        unsafe { p.restartComponent(RestartFlags_::kLatencyChanged) };
        assert_eq!(
            state.take_requests(),
            InstanceRequests {
                latency_changed: true,
                ..Default::default()
            }
        );
        // SAFETY: as above.
        unsafe {
            p.restartComponent(RestartFlags_::kReloadComponent | RestartFlags_::kParamValuesChanged)
        };
        let r = state.take_requests();
        assert!(r.restart && r.params_changed && !r.latency_changed);
    }
}
