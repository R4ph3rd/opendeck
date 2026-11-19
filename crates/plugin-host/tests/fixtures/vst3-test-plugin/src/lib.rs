//! Test fixture: tiny VST3 plugins written with the `vst3` crate. Not shipped.
//!
//! One library, three behaviours, chosen by the file stem of the module the
//! host loaded (found with `dladdr` / `GetModuleFileNameW`):
//!
//! - `gain.vst3`: factory with three effect classes, "Test Gain" (separate
//!   component and controller), "Test Single Gain" (one object
//!   implementing both) and "Test Mono Gain" (mono buses only, which the
//!   host must refuse);
//! - `crash.vst3`: the same classes, but `process` aborts;
//! - `hang.vst3`: the same classes, but `setActive(true)` never returns.
//!
//! The effect: output = input x 2 x gain (normalized parameter 0 of 0..1,
//! default 0.5, so unity). Parameters: 0 "Gain" (continuous, sample-accurate
//! inside a block), 1 "Mode" (stepped, 4 steps, echoed to the output
//! parameter changes), 2 "Resets" (read-only, hidden; counts
//! `setProcessing(false)` calls and reports `count / 1000` in the next
//! block). Latency 64 frames, tail 4800 frames. Component state is the gain
//! as 8 bytes (f64 LE), controller state the mode as 8 bytes.
//!
//! `opendeck_test_simulate_edit(id, value)` (exported) makes the controller
//! act like an editor: `beginEdit`, `performEdit`, `endEdit` on the host's
//! `IComponentHandler`, then `restartComponent(kLatencyChanged)`.
#![allow(unsafe_code, missing_docs)]
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]

use std::ffi::{CString, c_char, c_void};
use std::ptr;
use std::slice;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

use vst3::{Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*, uid};

const LATENCY: u32 = 64;
const TAIL: u32 = 4800;

const MODE_GAIN: u8 = 0;
const MODE_CRASH: u8 = 1;
const MODE_HANG: u8 = 2;
static MODE: AtomicU8 = AtomicU8::new(MODE_GAIN);

fn mode() -> u8 {
    MODE.load(Ordering::Relaxed)
}

// ---- finding our own file ----

#[cfg(unix)]
fn module_path() -> Option<String> {
    use std::ffi::{CStr, c_int};
    #[repr(C)]
    struct DlInfo {
        fname: *const c_char,
        fbase: *mut c_void,
        sname: *const c_char,
        saddr: *mut c_void,
    }
    unsafe extern "C" {
        fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int;
    }
    let mut info = DlInfo {
        fname: ptr::null(),
        fbase: ptr::null_mut(),
        sname: ptr::null(),
        saddr: ptr::null_mut(),
    };
    let here = module_path as *const c_void;
    // SAFETY: valid address of our own code and a valid out struct.
    if unsafe { dladdr(here, &mut info) } == 0 || info.fname.is_null() {
        return None;
    }
    // SAFETY: dladdr returns a NUL-terminated path.
    Some(
        unsafe { CStr::from_ptr(info.fname) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[cfg(windows)]
fn module_path() -> Option<String> {
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, addr: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, buf: *mut u16, size: u32) -> u32;
    }
    let mut module = ptr::null_mut();
    let here = module_path as *const c_void;
    // FROM_ADDRESS | UNCHANGED_REFCOUNT
    // SAFETY: valid address of our own code and out pointer.
    if unsafe { GetModuleHandleExW(4 | 2, here, &mut module) } == 0 {
        return None;
    }
    let mut buf = [0u16; 1024];
    // SAFETY: valid buffer and length.
    let n = unsafe { GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0).then(|| String::from_utf16_lossy(&buf[..n]))
}

fn detect_mode() {
    let stem = module_path()
        .and_then(|p| {
            std::path::Path::new(&p)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let m = match stem.as_str() {
        "crash" => MODE_CRASH,
        "hang" => MODE_HANG,
        _ => MODE_GAIN,
    };
    MODE.store(m, Ordering::Relaxed);
}

// ---- small helpers ----

fn copy_cstring(src: &str, dst: &mut [c_char]) {
    let bytes = CString::new(src).unwrap_or_default();
    let bytes = bytes.as_bytes_with_nul();
    for (s, d) in bytes.iter().zip(dst.iter_mut()) {
        *d = *s as c_char;
    }
    if bytes.len() > dst.len()
        && let Some(last) = dst.last_mut()
    {
        *last = 0;
    }
}

fn copy_wstring(src: &str, dst: &mut [TChar]) {
    let mut n = 0;
    for (s, d) in src.encode_utf16().zip(dst.iter_mut()) {
        *d = s as TChar;
        n += 1;
    }
    if n < dst.len() {
        dst[n] = 0;
    } else if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

unsafe fn write_f64(stream: *mut IBStream, v: f64) -> tresult {
    let Some(s) = (unsafe { ComRef::from_raw(stream) }) else {
        return kInvalidArgument;
    };
    let mut bytes = v.to_le_bytes();
    let mut written = 0;
    unsafe { s.write(bytes.as_mut_ptr().cast(), 8, &mut written) }
}

unsafe fn read_f64(stream: *mut IBStream) -> Option<f64> {
    let s = unsafe { ComRef::from_raw(stream) }?;
    let mut bytes = [0u8; 8];
    let mut read = 0;
    let r = unsafe { s.read(bytes.as_mut_ptr().cast(), 8, &mut read) };
    (r == kResultOk && read == 8).then(|| f64::from_le_bytes(bytes))
}

// ---- shared plugin logic ----

const GAIN: u32 = 0;
const MODE_PARAM: u32 = 1;
const RESETS: u32 = 2;

#[derive(Default)]
struct Core {
    /// What the DSP uses (f64 bits of the normalized gain).
    dsp_gain: AtomicU64,
    /// What the controller reports, per parameter id.
    ctrl: [AtomicU64; 3],
    resets: AtomicU32,
    reported: AtomicU32,
    processing: AtomicU8,
    handler: Mutex<Option<usize>>,
}

impl Core {
    fn new() -> Self {
        let c = Core::default();
        c.dsp_gain.store(0.5f64.to_bits(), Ordering::Relaxed);
        c.ctrl[0].store(0.5f64.to_bits(), Ordering::Relaxed);
        c
    }

    fn ctrl_get(&self, id: u32) -> f64 {
        self.ctrl
            .get(id as usize)
            .map_or(0.0, |a| f64::from_bits(a.load(Ordering::Relaxed)))
    }

    fn ctrl_set(&self, id: u32, v: f64) -> bool {
        match self.ctrl.get(id as usize) {
            Some(a) => {
                a.store(v.to_bits(), Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    unsafe fn process(&self, d: &ProcessData) -> tresult {
        if mode() == MODE_CRASH {
            std::process::abort();
        }
        let n = d.numSamples.max(0) as usize;
        // Parameter changes: gain points for the block, others echoed.
        let mut points = [(0i32, 0.0f64); 32];
        let mut count = 0;
        if let Some(changes) = unsafe { ComRef::from_raw(d.inputParameterChanges) } {
            for i in 0..unsafe { changes.getParameterCount() } {
                let Some(q) = (unsafe { ComRef::from_raw(changes.getParameterData(i)) }) else {
                    continue;
                };
                let id = unsafe { q.getParameterId() };
                for p in 0..unsafe { q.getPointCount() } {
                    let (mut offset, mut value) = (0, 0.0);
                    if unsafe { q.getPoint(p, &mut offset, &mut value) } != kResultTrue {
                        continue;
                    }
                    match id {
                        GAIN if count < points.len() => {
                            points[count] = (offset, value);
                            count += 1;
                        }
                        MODE_PARAM => unsafe { echo(d.outputParameterChanges, id, offset, value) },
                        _ => {}
                    }
                }
            }
        }
        let resets = self.resets.load(Ordering::Relaxed);
        if resets != self.reported.swap(resets, Ordering::Relaxed) {
            unsafe {
                echo(
                    d.outputParameterChanges,
                    RESETS,
                    0,
                    f64::from(resets) / 1000.0,
                )
            };
        }

        if d.numInputs < 1 || d.numOutputs < 1 || d.inputs.is_null() || d.outputs.is_null() {
            return kResultOk;
        }
        let (input, output) = unsafe { (&*d.inputs, &*d.outputs) };
        if input.numChannels != 2 || output.numChannels != 2 {
            return kResultOk;
        }
        let (ic, oc) = unsafe {
            (
                slice::from_raw_parts(input.__field0.channelBuffers32, 2),
                slice::from_raw_parts(output.__field0.channelBuffers32, 2),
            )
        };
        let mut gain = f64::from_bits(self.dsp_gain.load(Ordering::Relaxed));
        let mut next = 0;
        for i in 0..n {
            while next < count && points[next].0 as usize <= i {
                gain = points[next].1;
                next += 1;
            }
            let g = (2.0 * gain) as f32;
            for c in 0..2 {
                unsafe { *oc[c].add(i) = g * *ic[c].add(i) };
            }
        }
        self.dsp_gain.store(gain.to_bits(), Ordering::Relaxed);
        kResultOk
    }

    fn simulate_edit(&self, id: u32, value: f64) {
        let Some(h) = *self.handler.lock().unwrap() else {
            return;
        };
        // SAFETY: the pointer was given to `setComponentHandler` and the
        // host keeps the handler alive while the controller is.
        unsafe {
            if let Some(h) = ComRef::from_raw(h as *mut IComponentHandler) {
                h.beginEdit(id);
                h.performEdit(id, value);
                h.endEdit(id);
                h.restartComponent(RestartFlags_::kLatencyChanged);
            }
        }
    }
}

unsafe fn echo(out: *mut IParameterChanges, id: u32, offset: i32, value: f64) {
    let Some(list) = (unsafe { ComRef::from_raw(out) }) else {
        return;
    };
    let mut index = 0;
    if let Some(q) = unsafe { ComRef::from_raw(list.addParameterData(&id, &mut index)) } {
        let mut at = 0;
        unsafe { q.addPoint(offset, value, &mut at) };
    }
}

/// The last core created, for `opendeck_test_simulate_edit`.
static LAST_CORE: Mutex<Option<usize>> = Mutex::new(None);

trait HasCore {
    fn core(&self) -> &Core;
}

macro_rules! plugin_base {
    ($t:ty) => {
        impl IPluginBaseTrait for $t {
            unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
                kResultOk
            }
            unsafe fn terminate(&self) -> tresult {
                kResultOk
            }
        }
    };
}

macro_rules! component_impl {
    ($t:ty, $controller:expr, $channels:expr) => {
        impl IComponentTrait for $t {
            unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
                match $controller {
                    Some(cid) => {
                        unsafe { *class_id = cid };
                        kResultOk
                    }
                    None => kNotImplemented,
                }
            }
            unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
                kResultOk
            }
            unsafe fn getBusCount(&self, media: MediaType, dir: BusDirection) -> i32 {
                match media as MediaTypes {
                    MediaTypes_::kAudio => match dir as BusDirections {
                        BusDirections_::kInput | BusDirections_::kOutput => 1,
                        _ => 0,
                    },
                    _ => 0,
                }
            }
            unsafe fn getBusInfo(
                &self,
                media: MediaType,
                dir: BusDirection,
                index: i32,
                bus: *mut BusInfo,
            ) -> tresult {
                if media as MediaTypes != MediaTypes_::kAudio || index != 0 {
                    return kInvalidArgument;
                }
                let bus = unsafe { &mut *bus };
                bus.mediaType = MediaTypes_::kAudio as MediaType;
                bus.direction = dir;
                bus.channelCount = $channels;
                copy_wstring(
                    if dir as BusDirections == BusDirections_::kInput {
                        "Input"
                    } else {
                        "Output"
                    },
                    &mut bus.name,
                );
                bus.busType = BusTypes_::kMain as BusType;
                bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;
                kResultOk
            }
            unsafe fn getRoutingInfo(&self, _: *mut RoutingInfo, _: *mut RoutingInfo) -> tresult {
                kNotImplemented
            }
            unsafe fn activateBus(
                &self,
                _: MediaType,
                _: BusDirection,
                _: i32,
                _: TBool,
            ) -> tresult {
                kResultOk
            }
            unsafe fn setActive(&self, state: TBool) -> tresult {
                if state != 0 && mode() == MODE_HANG {
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
                kResultOk
            }
            unsafe fn setState(&self, state: *mut IBStream) -> tresult {
                match unsafe { read_f64(state) } {
                    Some(v) if v.is_finite() => {
                        self.core().dsp_gain.store(v.to_bits(), Ordering::Relaxed);
                        kResultOk
                    }
                    _ => kResultFalse,
                }
            }
            unsafe fn getState(&self, state: *mut IBStream) -> tresult {
                let v = f64::from_bits(self.core().dsp_gain.load(Ordering::Relaxed));
                unsafe { write_f64(state, v) }
            }
        }

        impl IAudioProcessorTrait for $t {
            unsafe fn setBusArrangements(
                &self,
                inputs: *mut SpeakerArrangement,
                num_ins: i32,
                outputs: *mut SpeakerArrangement,
                num_outs: i32,
            ) -> tresult {
                if num_ins != 1 || num_outs != 1 {
                    return kResultFalse;
                }
                if $channels != 2
                    || unsafe { *inputs != SpeakerArr::kStereo || *outputs != SpeakerArr::kStereo }
                {
                    return kResultFalse;
                }
                kResultTrue
            }
            unsafe fn getBusArrangement(
                &self,
                _dir: BusDirection,
                index: i32,
                arr: *mut SpeakerArrangement,
            ) -> tresult {
                if index == 0 {
                    unsafe {
                        *arr = if $channels == 2 {
                            SpeakerArr::kStereo
                        } else {
                            SpeakerArr::kMono
                        }
                    };
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
                if size as SymbolicSampleSizes == SymbolicSampleSizes_::kSample32 {
                    kResultOk
                } else {
                    kNotImplemented
                }
            }
            unsafe fn getLatencySamples(&self) -> u32 {
                LATENCY
            }
            unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
                kResultOk
            }
            unsafe fn setProcessing(&self, state: TBool) -> tresult {
                let was = self
                    .core()
                    .processing
                    .swap(u8::from(state != 0), Ordering::Relaxed);
                if state == 0 && was != 0 {
                    self.core().resets.fetch_add(1, Ordering::Relaxed);
                }
                kResultOk
            }
            unsafe fn process(&self, data: *mut ProcessData) -> tresult {
                unsafe { self.core().process(&*data) }
            }
            unsafe fn getTailSamples(&self) -> u32 {
                TAIL
            }
        }

        impl IProcessContextRequirementsTrait for $t {
            unsafe fn getProcessContextRequirements(&self) -> u32 {
                0
            }
        }
    };
}

macro_rules! controller_impl {
    ($t:ty) => {
        impl IEditControllerTrait for $t {
            unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
                match unsafe { read_f64(state) } {
                    Some(v) => {
                        self.core().ctrl_set(GAIN, v);
                        kResultOk
                    }
                    None => kResultFalse,
                }
            }
            unsafe fn setState(&self, state: *mut IBStream) -> tresult {
                match unsafe { read_f64(state) } {
                    Some(v) => {
                        self.core().ctrl_set(MODE_PARAM, v);
                        kResultOk
                    }
                    None => kResultFalse,
                }
            }
            unsafe fn getState(&self, state: *mut IBStream) -> tresult {
                unsafe { write_f64(state, self.core().ctrl_get(MODE_PARAM)) }
            }
            unsafe fn getParameterCount(&self) -> i32 {
                3
            }
            unsafe fn getParameterInfo(&self, index: i32, info: *mut ParameterInfo) -> tresult {
                let info = unsafe { &mut *info };
                let (id, title, steps, default, flags) = match index {
                    0 => (
                        GAIN,
                        "Gain",
                        0,
                        0.5,
                        ParameterInfo_::ParameterFlags_::kCanAutomate,
                    ),
                    1 => (
                        MODE_PARAM,
                        "Mode",
                        3,
                        0.0,
                        ParameterInfo_::ParameterFlags_::kCanAutomate,
                    ),
                    2 => (
                        RESETS,
                        "Resets",
                        0,
                        0.0,
                        ParameterInfo_::ParameterFlags_::kIsReadOnly
                            | ParameterInfo_::ParameterFlags_::kIsHidden,
                    ),
                    _ => return kInvalidArgument,
                };
                info.id = id;
                copy_wstring(title, &mut info.title);
                copy_wstring(title, &mut info.shortTitle);
                copy_wstring("", &mut info.units);
                info.stepCount = steps;
                info.defaultNormalizedValue = default;
                info.unitId = 0;
                info.flags = flags as i32;
                kResultOk
            }
            unsafe fn getParamStringByValue(
                &self,
                id: u32,
                v: f64,
                string: *mut String128,
            ) -> tresult {
                let text = match id {
                    GAIN => format!("{:.2} x", 2.0 * v),
                    MODE_PARAM => format!("Mode {}", (v * 3.0).round() as i32),
                    RESETS => format!("{}", (v * 1000.0).round() as i32),
                    _ => return kInvalidArgument,
                };
                copy_wstring(&text, unsafe { &mut *string });
                kResultOk
            }
            unsafe fn getParamValueByString(&self, _: u32, _: *mut TChar, _: *mut f64) -> tresult {
                kNotImplemented
            }
            unsafe fn normalizedParamToPlain(&self, id: u32, v: f64) -> f64 {
                match id {
                    GAIN => 2.0 * v,
                    MODE_PARAM => (v * 3.0).round(),
                    RESETS => v * 1000.0,
                    _ => 0.0,
                }
            }
            unsafe fn plainParamToNormalized(&self, id: u32, plain: f64) -> f64 {
                match id {
                    GAIN => plain / 2.0,
                    MODE_PARAM => plain / 3.0,
                    RESETS => plain / 1000.0,
                    _ => 0.0,
                }
            }
            unsafe fn getParamNormalized(&self, id: u32) -> f64 {
                self.core().ctrl_get(id)
            }
            unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
                if self.core().ctrl_set(id, value) {
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
                *self.core().handler.lock().unwrap() =
                    (!handler.is_null()).then_some(handler as usize);
                kResultOk
            }
            unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
                ptr::null_mut()
            }
        }
    };
}

// ---- class 1: separate component and controller ----

const PROCESSOR_CID: TUID = uid(0x6A31_7000, 0x0000_4000, 0x8000_0000, 0x0000_0001);
const CONTROLLER_CID: TUID = uid(0x6A31_7000, 0x0000_4000, 0x8000_0000, 0x0000_0002);
const SINGLE_CID: TUID = uid(0x6A31_7000, 0x0000_4000, 0x8000_0000, 0x0000_0003);

struct Processor {
    core: Core,
}

impl Class for Processor {
    type Interfaces = (IComponent, IAudioProcessor, IProcessContextRequirements);
}

impl HasCore for Processor {
    fn core(&self) -> &Core {
        &self.core
    }
}

plugin_base!(Processor);
component_impl!(Processor, Some(CONTROLLER_CID), 2);

/// The controller shares its logic with the processor but has its own
/// values, like a real separate controller.
struct Controller {
    core: Core,
}

impl Class for Controller {
    type Interfaces = (IEditController,);
}

impl HasCore for Controller {
    fn core(&self) -> &Core {
        &self.core
    }
}

plugin_base!(Controller);
controller_impl!(Controller);

// ---- class 2: one object for both ----

struct Single {
    core: Core,
}

impl Class for Single {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl HasCore for Single {
    fn core(&self) -> &Core {
        &self.core
    }
}

plugin_base!(Single);
component_impl!(Single, None::<TUID>, 2);
controller_impl!(Single);

// ---- class 3: a mono plugin, which the host must refuse ----

const MONO_CID: TUID = uid(0x6A31_7000, 0x0000_4000, 0x8000_0000, 0x0000_0004);

struct Mono {
    core: Core,
}

impl Class for Mono {
    type Interfaces = (
        IComponent,
        IAudioProcessor,
        IProcessContextRequirements,
        IEditController,
    );
}

impl HasCore for Mono {
    fn core(&self) -> &Core {
        &self.core
    }
}

plugin_base!(Mono);
component_impl!(Mono, None::<TUID>, 1);
controller_impl!(Mono);

// The two interfaces of `Single` share method names (`getState`, `setState`);
// the COM wrapper dispatches through the right trait for each vtable.

// ---- factory ----

struct Factory;

impl Class for Factory {
    type Interfaces = (IPluginFactory3,);
}

struct ClassDesc {
    cid: TUID,
    category: &'static str,
    name: &'static str,
    sub: &'static str,
}

const CLASSES: [ClassDesc; 4] = [
    ClassDesc {
        cid: PROCESSOR_CID,
        category: "Audio Module Class",
        name: "Test Gain",
        sub: "Fx|Dynamics",
    },
    ClassDesc {
        cid: CONTROLLER_CID,
        category: "Component Controller Class",
        name: "Test Gain",
        sub: "",
    },
    ClassDesc {
        cid: SINGLE_CID,
        category: "Audio Module Class",
        name: "Test Single Gain",
        sub: "Fx",
    },
    ClassDesc {
        cid: MONO_CID,
        category: "Audio Module Class",
        name: "Test Mono Gain",
        sub: "Fx",
    },
];

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        let info = unsafe { &mut *info };
        copy_cstring("OpenDeck tests", &mut info.vendor);
        copy_cstring("https://example.com", &mut info.url);
        copy_cstring("tests@example.com", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as int32;
        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        CLASSES.len() as i32
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        let Some(c) = CLASSES.get(index as usize) else {
            return kInvalidArgument;
        };
        let info = unsafe { &mut *info };
        info.cid = c.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_cstring(c.category, &mut info.category);
        copy_cstring(c.name, &mut info.name);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        let cid = unsafe { *(cid as *const TUID) };
        let unknown = if cid == PROCESSOR_CID {
            let p = ComWrapper::new(Processor { core: Core::new() });
            *LAST_CORE.lock().unwrap() = Some(&p.core as *const Core as usize);
            p.to_com_ptr::<FUnknown>()
        } else if cid == CONTROLLER_CID {
            let c = ComWrapper::new(Controller { core: Core::new() });
            *LAST_CORE.lock().unwrap() = Some(&c.core as *const Core as usize);
            c.to_com_ptr::<FUnknown>()
        } else if cid == SINGLE_CID {
            let s = ComWrapper::new(Single { core: Core::new() });
            *LAST_CORE.lock().unwrap() = Some(&s.core as *const Core as usize);
            s.to_com_ptr::<FUnknown>()
        } else if cid == MONO_CID {
            ComWrapper::new(Mono { core: Core::new() }).to_com_ptr::<FUnknown>()
        } else {
            None
        };
        match unknown {
            Some(u) => {
                let p = u.as_ptr();
                unsafe { ((*(*p).vtbl).queryInterface)(p, iid as *mut TUID, obj) }
            }
            None => kInvalidArgument,
        }
    }
}

impl IPluginFactory2Trait for Factory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        let Some(c) = CLASSES.get(index as usize) else {
            return kInvalidArgument;
        };
        let info = unsafe { &mut *info };
        info.cid = c.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_cstring(c.category, &mut info.category);
        copy_cstring(c.name, &mut info.name);
        info.classFlags = 0;
        copy_cstring(c.sub, &mut info.subCategories);
        copy_cstring("OpenDeck tests", &mut info.vendor);
        copy_cstring("1.2.3", &mut info.version);
        copy_cstring("VST 3.7", &mut info.sdkVersion);
        kResultOk
    }
}

impl IPluginFactory3Trait for Factory {
    unsafe fn getClassInfoUnicode(&self, index: i32, info: *mut PClassInfoW) -> tresult {
        let Some(c) = CLASSES.get(index as usize) else {
            return kInvalidArgument;
        };
        let info = unsafe { &mut *info };
        info.cid = c.cid;
        info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
        copy_cstring(c.category, &mut info.category);
        copy_wstring(c.name, &mut info.name);
        info.classFlags = 0;
        copy_cstring(c.sub, &mut info.subCategories);
        copy_wstring("OpenDeck tests", &mut info.vendor);
        copy_wstring("1.2.3", &mut info.version);
        copy_wstring("VST 3.7", &mut info.sdkVersion);
        kResultOk
    }

    unsafe fn setHostContext(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }
}

// ---- module entry points ----

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "system" fn InitDll() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "system" fn ExitDll() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "system" fn bundleEntry(_bundle_ref: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[unsafe(no_mangle)]
extern "system" fn bundleExit() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "system" fn ModuleEntry(_library_handle: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "linux")]
#[unsafe(no_mangle)]
extern "system" fn ModuleExit() -> bool {
    true
}

#[unsafe(no_mangle)]
extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    detect_mode();
    ComWrapper::new(Factory)
        .to_com_ptr::<IPluginFactory>()
        .map_or(ptr::null_mut(), ComPtr::into_raw)
}

/// Makes the most recently created controller act like an editor (see the
/// crate docs). For tests that load this library themselves.
#[unsafe(no_mangle)]
pub extern "C" fn opendeck_test_simulate_edit(id: u32, value: f64) {
    let core = *LAST_CORE.lock().unwrap();
    if let Some(p) = core {
        // SAFETY: the pointer is a `Core` inside a live COM object; tests
        // call this while the instance exists.
        unsafe { &*(p as *const Core) }.simulate_edit(id, value);
    }
}
