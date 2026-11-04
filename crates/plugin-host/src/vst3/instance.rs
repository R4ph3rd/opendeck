//! The main-thread half of a VST3 instance: component, audio processor and
//! edit controller.
//!
//! Creation order, from the VST 3 workflow documentation
//! (<https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/API+Documentation/Index.html>):
//! `createInstance(IComponent)`, `initialize(host context)`, query
//! `IAudioProcessor`, then the controller: the component itself when it
//! implements `IEditController` (single-component plugins), else the class
//! from `getControllerClassId`, created and initialised separately. The
//! controller gets our `IComponentHandler`; if both halves implement
//! `IConnectionPoint` they are connected to each other. The component's
//! state is sent to a separate controller once (`setComponentState`).
//!
//! Activation (every call off the audio thread): `setBusArrangements`
//! (stereo main buses), `activateBus`, `setupProcessing` (engine rate,
//! maximum block), `setActive(true)`, `setProcessing(true)`; latency and
//! tail are read after that. Deactivation reverses it.

use std::path::Path;
use std::sync::Arc;

use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::{ParamDescriptor, ParamId};
use opendeck_contracts::plugin::{PluginInfo, PluginKind};
use vst3::Steinberg::Vst::{
    BusDirections_, IAudioProcessor, IAudioProcessorTrait, IComponent, IComponentTrait,
    IConnectionPoint, IConnectionPointTrait, IEditController, IEditControllerTrait, IoModes_,
    MediaTypes_, ProcessModes_, ProcessSetup, SpeakerArr, SpeakerArrangement, String128,
    SymbolicSampleSizes_,
};
use vst3::Steinberg::{IPluginBaseTrait, TUID, kNotImplemented};
use vst3::{ComPtr, ComWrapper};

use super::events::EventList;
use super::handler::{ComponentHandler, EditEvent, HandlerState};
use super::module::Module;
use super::param_changes::ParamChanges;
use super::params::{ParamEntry, decode_state, encode_state, read_params};
use super::processor::{Layout, Vst3Processor};
use super::stream::MemStream;
use super::util::{e, hex_to_tuid, is_ok, utf16_to_string};
use crate::PluginError;
use crate::backend::{
    HostWake, InstanceRequests, MAX_PARAM_EVENTS, PARAM_RING, ParamChange, PluginInstance,
    PluginProcessor,
};

/// `IAudioProcessor::kInfiniteTail`.
const INFINITE_TAIL: u32 = u32::MAX;

/// One VST3 plugin instance. Lives on the main thread.
pub struct Vst3Instance {
    // Field order: COM pointers drop before the module that holds the code.
    sync_rx: Option<rtrb::Consumer<ParamChange>>,
    handler_obj: ComWrapper<ComponentHandler>,
    handler: Arc<HandlerState>,
    connections: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    controller: Option<ComPtr<IEditController>>,
    separate_controller: bool,
    audio: ComPtr<IAudioProcessor>,
    component: ComPtr<IComponent>,
    layout: Layout,
    params: Arc<[ParamEntry]>,
    settable: Arc<[u32]>,
    kind: PluginKind,
    active: bool,
    latency: u32,
    tail: Option<u32>,
    module: Arc<Module>,
}

impl Vst3Instance {
    /// Loads `info.path` and creates the plugin. Main thread.
    pub fn create(info: &PluginInfo, wake: HostWake) -> Result<Self, PluginError> {
        let module = Module::load(Path::new(&info.path))?;
        let cid = hex_to_tuid(&info.key.id).ok_or_else(|| PluginError::UnknownPlugin {
            provider: info.key.provider.clone(),
            id: info.key.id.clone(),
        })?;
        let component: ComPtr<IComponent> = module.create(&cid)?;
        let ctx = module.host_context();
        // SAFETY: the component is live; `ctx` outlives the call.
        if !is_ok(unsafe { component.initialize(ctx) }) {
            return Err(PluginError::Plugin("component initialize failed".into()));
        }
        let Some(audio) = component.cast::<IAudioProcessor>() else {
            // SAFETY: live component, initialised above.
            let _ = unsafe { component.terminate() };
            return Err(PluginError::Plugin(
                "class has no IAudioProcessor interface".into(),
            ));
        };
        let handler = Arc::new(HandlerState::default());
        let mut this = Self {
            sync_rx: None,
            handler_obj: ComponentHandler::new(handler.clone(), wake),
            handler,
            connections: None,
            controller: None,
            separate_controller: false,
            audio,
            component,
            layout: Layout::default(),
            params: Arc::from(Vec::new()),
            settable: Arc::from(Vec::new()),
            kind: info.kind,
            active: false,
            latency: 0,
            tail: None,
            module,
        };
        // From here a failure drops `this`, whose `Drop` terminates what was
        // initialised.
        // SAFETY: live component.
        let _ = unsafe { this.component.setIoMode(e(IoModes_::kSimple)) };
        this.attach_controller(&ctx)?;
        this.layout = Layout::read(&this.component, info.kind)?;
        this.negotiate_buses()?;
        let params: Vec<ParamEntry> = this
            .controller
            .as_ref()
            .map(read_params)
            .unwrap_or_default();
        this.settable = params
            .iter()
            .filter(|e| e.settable)
            .map(|e| e.desc.id.0)
            .collect::<Vec<_>>()
            .into();
        this.params = params.into();
        Ok(this)
    }

    fn attach_controller(
        &mut self,
        ctx: &*mut vst3::Steinberg::FUnknown,
    ) -> Result<(), PluginError> {
        let (controller, separate) = match self.component.cast::<IEditController>() {
            Some(c) => (Some(c), false),
            None => {
                let mut cid: TUID = [0; 16];
                // SAFETY: live component, valid out pointer.
                let found = is_ok(unsafe { self.component.getControllerClassId(&mut cid) });
                if found {
                    let c: ComPtr<IEditController> = self.module.create(&cid)?;
                    // SAFETY: live controller; the host context outlives the call.
                    if !is_ok(unsafe { c.initialize(*ctx) }) {
                        return Err(PluginError::Plugin("controller initialize failed".into()));
                    }
                    (Some(c), true)
                } else {
                    (None, false)
                }
            }
        };
        self.separate_controller = separate;
        let Some(controller) = controller else {
            return Ok(());
        };
        // SAFETY: live controller and handler.
        let _ =
            unsafe { controller.setComponentHandler(ComponentHandler::as_ptr(&self.handler_obj)) };
        if separate {
            if let (Some(a), Some(b)) = (
                self.component.cast::<IConnectionPoint>(),
                controller.cast::<IConnectionPoint>(),
            ) {
                // SAFETY: both live; each side is told about the other.
                unsafe {
                    let _ = a.connect(b.as_ptr());
                    let _ = b.connect(a.as_ptr());
                }
                self.connections = Some((a, b));
            }
            // The controller starts from the component's default state.
            let stream = MemStream::empty();
            // SAFETY: live objects, valid stream pointer.
            unsafe {
                if is_ok(self.component.getState(MemStream::as_ptr(&stream))) {
                    stream.rewind();
                    let _ = controller.setComponentState(MemStream::as_ptr(&stream));
                }
            }
        }
        self.controller = Some(controller);
        Ok(())
    }

    /// Sets the stereo arrangement on the main buses and activates exactly
    /// the buses we feed: the main audio buses, and the first event input
    /// of an instrument. Component inactive.
    fn negotiate_buses(&self) -> Result<(), PluginError> {
        let l = &self.layout;
        let mut ins: Vec<SpeakerArrangement> = l.current_arrangements(&self.audio, true);
        let mut outs: Vec<SpeakerArrangement> = l.current_arrangements(&self.audio, false);
        if let Some(a) = l.main_in.and_then(|i| ins.get_mut(i)) {
            *a = SpeakerArr::kStereo;
        }
        if let Some(a) = outs.get_mut(l.main_out) {
            *a = SpeakerArr::kStereo;
        }
        // SAFETY: arrays of the declared lengths; plain calls on live objects.
        unsafe {
            let _ = self.audio.setBusArrangements(
                ins.as_mut_ptr(),
                ins.len() as i32,
                outs.as_mut_ptr(),
                outs.len() as i32,
            );
        }
        // Whatever the plugin answered, the main buses must now be stereo.
        let stereo = |input: bool, index: usize| {
            let mut arr: SpeakerArrangement = 0;
            let dir = e(if input {
                BusDirections_::kInput
            } else {
                BusDirections_::kOutput
            });
            // SAFETY: valid out pointer.
            is_ok(unsafe { self.audio.getBusArrangement(dir, index as i32, &mut arr) })
                && arr == SpeakerArr::kStereo
        };
        if l.main_in.is_some_and(|i| !stereo(true, i)) || !stereo(false, l.main_out) {
            return Err(PluginError::Plugin(
                "no stereo main audio bus (mono and surround plugins are not supported)".into(),
            ));
        }
        for (input, buses) in [(true, &l.audio_in), (false, &l.audio_out)] {
            let dir = e(if input {
                BusDirections_::kInput
            } else {
                BusDirections_::kOutput
            });
            for (i, bus) in buses.iter().enumerate() {
                // SAFETY: plain call on a live component.
                let _ = unsafe {
                    self.component.activateBus(
                        e(MediaTypes_::kAudio),
                        dir,
                        i as i32,
                        u8::from(bus.is_main),
                    )
                };
            }
        }
        if l.event_inputs > 0 {
            // SAFETY: plain call on a live component.
            let _ = unsafe {
                self.component.activateBus(
                    e(MediaTypes_::kEvent),
                    e(BusDirections_::kInput),
                    0,
                    u8::from(self.kind == PluginKind::Instrument),
                )
            };
        }
        Ok(())
    }

    /// Drains values the processor mirrored (host changes and plugin output)
    /// into the controller, so its values and `getState` are current.
    fn sync_controller(&mut self) {
        let (Some(rx), Some(controller)) = (self.sync_rx.as_mut(), self.controller.as_ref()) else {
            return;
        };
        while let Ok(c) = rx.pop() {
            // SAFETY: plain call on a live controller.
            let _ = unsafe { controller.setParamNormalized(c.id.0, f64::from(c.normalized)) };
        }
    }

    /// The controller's plain (display) value for a normalized one
    /// (`normalizedParamToPlain`). `None` without a controller.
    pub fn normalized_to_plain(&self, id: ParamId, normalized: f64) -> Option<f64> {
        let c = self.controller.as_ref()?;
        // SAFETY: plain call on a live controller.
        Some(unsafe { c.normalizedParamToPlain(id.0, normalized.clamp(0.0, 1.0)) })
    }

    /// The normalized value of a plain one (`plainParamToNormalized`).
    pub fn plain_to_normalized(&self, id: ParamId, plain: f64) -> Option<f64> {
        let c = self.controller.as_ref()?;
        // SAFETY: plain call on a live controller.
        Some(unsafe { c.plainParamToNormalized(id.0, plain) })
    }

    /// The plugin's text for a normalized value (`getParamStringByValue`),
    /// such as `"-6.0 dB"`.
    pub fn value_text(&self, id: ParamId, normalized: f64) -> Option<String> {
        let c = self.controller.as_ref()?;
        let mut text: String128 = [0; 128];
        // SAFETY: valid out pointer to a String128.
        let ok =
            is_ok(unsafe { c.getParamStringByValue(id.0, normalized.clamp(0.0, 1.0), &mut text) });
        ok.then(|| utf16_to_string(&text))
    }

    /// Takes the controller's edit gestures since the last call (the user
    /// moving controls in the plugin's editor). The UI uses them for the
    /// preset dirty flag and later for automation recording.
    pub fn take_edits(&mut self) -> Vec<EditEvent> {
        self.handler.take_edits()
    }

    /// True if the class has a separate edit controller object.
    pub fn has_separate_controller(&self) -> bool {
        self.separate_controller
    }

    fn state_part(
        &self,
        f: impl FnOnce(*mut vst3::Steinberg::IBStream) -> i32,
    ) -> Result<Vec<u8>, PluginError> {
        let stream = MemStream::empty();
        let result = f(MemStream::as_ptr(&stream));
        match result {
            r if is_ok(r) => Ok(stream.take()),
            r if r == kNotImplemented => Ok(Vec::new()),
            r => Err(PluginError::Plugin(format!("getState failed (result {r})"))),
        }
    }
}

impl PluginInstance for Vst3Instance {
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
        if self.active {
            return Err(PluginError::Plugin("already active".into()));
        }
        let max = i32::try_from(max_block_frames)
            .map_err(|_| PluginError::Plugin("block size too large".into()))?;
        self.negotiate_buses()?;
        let mut setup = ProcessSetup {
            processMode: e(ProcessModes_::kRealtime),
            symbolicSampleSize: e(SymbolicSampleSizes_::kSample32),
            maxSamplesPerBlock: max,
            sampleRate: sample_rate.as_f64(),
        };
        // SAFETY: plain calls on live objects, valid setup pointer. All of
        // this runs on the main thread, never on the audio thread.
        unsafe {
            if !is_ok(
                self.audio
                    .canProcessSampleSize(e(SymbolicSampleSizes_::kSample32)),
            ) {
                return Err(PluginError::Plugin("no 32-bit float processing".into()));
            }
            if !is_ok(self.audio.setupProcessing(&mut setup)) {
                return Err(PluginError::Plugin("setupProcessing failed".into()));
            }
            if !is_ok(self.component.setActive(1)) {
                return Err(PluginError::Plugin("setActive(true) failed".into()));
            }
            let started = self.audio.setProcessing(1);
            if !is_ok(started) && started != kNotImplemented {
                let _ = self.component.setActive(0);
                return Err(PluginError::Plugin("setProcessing(true) failed".into()));
            }
        }
        self.active = true;
        // SAFETY: plain calls on a live, active audio processor.
        self.latency = unsafe { self.audio.getLatencySamples() };
        // VST3 has one value for "no tail" and "not said" (0); plugins with
        // tails that forget to report it are common, so 0 means unknown and
        // the slot keeps processing until silence (-90 dBFS).
        self.tail = match unsafe { self.audio.getTailSamples() } {
            0 => None,
            INFINITE_TAIL => Some(u32::MAX),
            n => Some(n),
        };

        let (edit_tx, edit_rx) = rtrb::RingBuffer::<ParamChange>::new(PARAM_RING);
        let (sync_tx, sync_rx) = rtrb::RingBuffer::<ParamChange>::new(PARAM_RING);
        self.handler.set_audio_ring(Some(edit_tx));
        self.sync_rx = Some(sync_rx);
        let processor = Vst3Processor::new(
            self.audio.clone(),
            self.module.clone(),
            &self.layout,
            self.kind == PluginKind::Effect,
            max_block_frames,
            ParamChanges::new(MAX_PARAM_EVENTS),
            ParamChanges::new(super::processor::MAX_OUT_PARAMS),
            EventList::new(),
            EventList::new(),
            self.settable.clone(),
            edit_rx,
            sync_tx,
        );
        Ok(Box::new(processor))
    }

    fn deactivate(&mut self, processor: Box<dyn PluginProcessor>) {
        // The audio thread let go of the processor; release its references
        // before the component deactivates.
        drop(processor);
        self.handler.set_audio_ring(None);
        self.sync_controller();
        self.sync_rx = None;
        if self.active {
            // SAFETY: plain calls on live objects, main thread.
            unsafe {
                let _ = self.audio.setProcessing(0);
                let _ = self.component.setActive(0);
            }
            self.active = false;
        }
        self.latency = 0;
    }

    fn latency_frames(&self) -> u32 {
        self.latency
    }

    fn tail_frames(&self) -> Option<u32> {
        self.tail
    }

    fn service(&mut self) -> InstanceRequests {
        self.sync_controller();
        let requests = self.handler.take_requests();
        if requests.latency_changed && self.active {
            // SAFETY: plain call on a live, active audio processor.
            self.latency = unsafe { self.audio.getLatencySamples() };
        }
        requests
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.sync_controller();
        let component = self.state_part(|s| {
            // SAFETY: live component; `s` is a valid stream pointer.
            unsafe { self.component.getState(s) }
        })?;
        let controller = match &self.controller {
            // For a single-component plugin the controller is the component,
            // and its controller-side state is separate (`IEditController::getState`).
            Some(c) => self.state_part(|s| {
                // SAFETY: live controller; `s` is a valid stream pointer.
                unsafe { c.getState(s) }
            })?,
            None => Vec::new(),
        };
        Ok(encode_state(&component, &controller))
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        let (component, controller) = decode_state(state)?;
        if !component.is_empty() {
            let stream = MemStream::with_bytes(component.to_vec());
            // SAFETY: live component; valid stream pointer.
            let r = unsafe { self.component.setState(MemStream::as_ptr(&stream)) };
            if !is_ok(r) && r != kNotImplemented {
                return Err(PluginError::Plugin(format!(
                    "component setState failed (result {r})"
                )));
            }
        }
        if let Some(c) = &self.controller {
            if !component.is_empty() {
                // The controller learns the processor's new values.
                let stream = MemStream::with_bytes(component.to_vec());
                // SAFETY: live controller; valid stream pointer.
                let _ = unsafe { c.setComponentState(MemStream::as_ptr(&stream)) };
            }
            if !controller.is_empty() {
                let stream = MemStream::with_bytes(controller.to_vec());
                // SAFETY: live controller; valid stream pointer.
                let r = unsafe { c.setState(MemStream::as_ptr(&stream)) };
                if !is_ok(r) && r != kNotImplemented {
                    return Err(PluginError::Plugin(format!(
                        "controller setState failed (result {r})"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Drop for Vst3Instance {
    fn drop(&mut self) {
        // SAFETY: plain calls on live objects, on the main thread, in the
        // order the VST 3 workflow documents for shutdown.
        unsafe {
            if self.active {
                let _ = self.audio.setProcessing(0);
                let _ = self.component.setActive(0);
            }
            if let Some((a, b)) = self.connections.take() {
                let _ = a.disconnect(b.as_ptr());
                let _ = b.disconnect(a.as_ptr());
            }
            if let Some(c) = self.controller.take() {
                let _ = c.setComponentHandler(std::ptr::null_mut());
                if self.separate_controller {
                    let _ = c.terminate();
                }
            }
            let _ = self.component.terminate();
        }
    }
}
