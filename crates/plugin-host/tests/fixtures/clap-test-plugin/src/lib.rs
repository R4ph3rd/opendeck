//! Test fixture for `opendeck-plugin-host`: a tiny CLAP plugin library written
//! directly against `clap-sys` (no host crate, no SDK), so the host tests load
//! it like any third-party `.clap` file.
//!
//! One library, three behaviours, chosen by the file stem the host passes to
//! `clap_entry.init` (the tests copy the built library to three names):
//!
//! - `crash.clap`: plugin `dev.opendeck.test.crash`, aborts in `process`;
//! - `hang.clap`: plugin `dev.opendeck.test.hang`, never returns from `activate`;
//! - anything else: plugin `dev.opendeck.test.gain`, a stereo gain.
//!
//! The gain plugin implements the extensions the host uses:
//!
//! - `audio-ports`: one stereo main input and output.
//! - `params`: id 0 "Gain" (0 to 2, default 1, applied sample-accurately at
//!   the event time), id 1 "Mode" (stepped 0 to 3, echoed back as an output
//!   event), id 2 "Resets" (read-only, hidden, the number of `reset` calls,
//!   reported as an output event at the next block).
//! - `latency`: [`LATENCY_FRAMES`] (reported only, the audio is not delayed).
//! - `tail`: [`TAIL_FRAMES`].
//! - `state`: gain and mode as two little-endian `f32`.
//!
//! The format follows the CLAP headers (`clap/plugin.h`, `ext/params.h`,
//! `ext/state.h`, `ext/audio-ports.h`) as mirrored by `clap-sys` 0.5.

#![allow(unsafe_code, missing_docs, clippy::missing_safety_doc)]

use std::ffi::{CStr, c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU8, Ordering};

use clap_sys::audio_buffer::clap_audio_buffer;
use clap_sys::entry::clap_plugin_entry;
use clap_sys::events::{
    CLAP_CORE_EVENT_SPACE_ID, CLAP_EVENT_PARAM_VALUE, clap_event_header, clap_event_param_value,
    clap_input_events, clap_output_events,
};
use clap_sys::ext::audio_ports::{
    CLAP_AUDIO_PORT_IS_MAIN, CLAP_EXT_AUDIO_PORTS, CLAP_PORT_STEREO, clap_audio_port_info,
    clap_plugin_audio_ports,
};
use clap_sys::ext::latency::{CLAP_EXT_LATENCY, clap_plugin_latency};
use clap_sys::ext::params::{
    CLAP_EXT_PARAMS, CLAP_PARAM_IS_AUTOMATABLE, CLAP_PARAM_IS_HIDDEN, CLAP_PARAM_IS_READONLY,
    CLAP_PARAM_IS_STEPPED, clap_param_info, clap_plugin_params,
};
use clap_sys::ext::state::{CLAP_EXT_STATE, clap_plugin_state};
use clap_sys::ext::tail::{CLAP_EXT_TAIL, clap_plugin_tail};
use clap_sys::factory::plugin_factory::{CLAP_PLUGIN_FACTORY_ID, clap_plugin_factory};
use clap_sys::host::clap_host;
use clap_sys::id::CLAP_INVALID_ID;
use clap_sys::plugin::{clap_plugin, clap_plugin_descriptor};
use clap_sys::process::{CLAP_PROCESS_CONTINUE, CLAP_PROCESS_ERROR, clap_process};
use clap_sys::stream::{clap_istream, clap_ostream};
use clap_sys::version::CLAP_VERSION;

/// Latency the gain plugin reports.
pub const LATENCY_FRAMES: u32 = 64;
/// Tail the gain plugin reports.
pub const TAIL_FRAMES: u32 = 4800;

const MODE_GAIN: u8 = 0;
const MODE_CRASH: u8 = 1;
const MODE_HANG: u8 = 2;
static MODE: AtomicU8 = AtomicU8::new(MODE_GAIN);

const PARAM_GAIN: u32 = 0;
const PARAM_MODE: u32 = 1;
const PARAM_RESETS: u32 = 2;

struct Features([*const c_char; 3]);
// SAFETY: points at static C string literals.
unsafe impl Sync for Features {}
static FEATURES: Features = Features([c"audio-effect".as_ptr(), c"stereo".as_ptr(), ptr::null()]);

const fn descriptor(id: &'static CStr, name: &'static CStr) -> clap_plugin_descriptor {
    clap_plugin_descriptor {
        clap_version: CLAP_VERSION,
        id: id.as_ptr(),
        name: name.as_ptr(),
        vendor: c"OpenDeck tests".as_ptr(),
        url: c"".as_ptr(),
        manual_url: c"".as_ptr(),
        support_url: c"".as_ptr(),
        version: c"1.2.3".as_ptr(),
        description: c"Test fixture".as_ptr(),
        features: FEATURES.0.as_ptr(),
    }
}

static DESCRIPTORS: [clap_plugin_descriptor; 3] = [
    descriptor(c"dev.opendeck.test.gain", c"Test Gain"),
    descriptor(c"dev.opendeck.test.crash", c"Test Crash"),
    descriptor(c"dev.opendeck.test.hang", c"Test Hang"),
];

fn current_descriptor() -> &'static clap_plugin_descriptor {
    &DESCRIPTORS[usize::from(MODE.load(Ordering::Relaxed)).min(2)]
}

struct State {
    gain: f32,
    mode: f32,
    resets: u32,
    resets_reported: u32,
}

#[repr(C)]
struct Instance {
    raw: clap_plugin,
    state: State,
}

unsafe fn state<'a>(p: *const clap_plugin) -> &'a mut State {
    // SAFETY: `plugin_data` points at `Instance::state`, alive until `destroy`.
    unsafe { &mut *((*p).plugin_data.cast::<State>()) }
}

unsafe extern "C" fn plugin_init(_: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn plugin_destroy(p: *const clap_plugin) {
    // SAFETY: `p` is the `Instance` leaked in `create_plugin`; `raw` is its first field.
    drop(unsafe { Box::from_raw(p.cast_mut().cast::<Instance>()) });
}

unsafe extern "C" fn plugin_activate(_: *const clap_plugin, rate: f64, min: u32, max: u32) -> bool {
    if MODE.load(Ordering::Relaxed) == MODE_HANG {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    rate > 0.0 && min >= 1 && max >= min
}

unsafe extern "C" fn plugin_deactivate(_: *const clap_plugin) {}

unsafe extern "C" fn plugin_start(_: *const clap_plugin) -> bool {
    true
}

unsafe extern "C" fn plugin_stop(_: *const clap_plugin) {}

unsafe extern "C" fn plugin_reset(p: *const clap_plugin) {
    // SAFETY: valid plugin pointer from the host.
    unsafe { state(p) }.resets += 1;
}

unsafe extern "C" fn plugin_on_main_thread(_: *const clap_plugin) {}

/// Applies one input event; returns true if it was a parameter value event.
unsafe fn apply_event(s: &mut State, h: *const clap_event_header, out: *const clap_output_events) {
    // SAFETY: the host passes valid event headers and output lists.
    unsafe {
        if (*h).space_id != CLAP_CORE_EVENT_SPACE_ID || (*h).type_ != CLAP_EVENT_PARAM_VALUE {
            return;
        }
        let e = &*h.cast::<clap_event_param_value>();
        match e.param_id {
            PARAM_GAIN => s.gain = e.value as f32,
            PARAM_MODE => {
                s.mode = e.value as f32;
                push_param(out, e.header.time, PARAM_MODE, e.value);
            }
            _ => {}
        }
    }
}

unsafe fn push_param(out: *const clap_output_events, time: u32, id: u32, value: f64) {
    let ev = clap_event_param_value {
        header: clap_event_header {
            size: size_of::<clap_event_param_value>() as u32,
            time,
            space_id: CLAP_CORE_EVENT_SPACE_ID,
            type_: CLAP_EVENT_PARAM_VALUE,
            flags: 0,
        },
        param_id: id,
        cookie: ptr::null_mut(),
        note_id: -1,
        port_index: -1,
        channel: -1,
        key: -1,
        value,
    };
    // SAFETY: `out` is a valid output list for this call.
    unsafe {
        if let Some(push) = (*out).try_push {
            push(out, &ev.header);
        }
    }
}

unsafe extern "C" fn plugin_process(p: *const clap_plugin, process: *const clap_process) -> i32 {
    if MODE.load(Ordering::Relaxed) == MODE_CRASH {
        std::process::abort();
    }
    // SAFETY: the host passes a valid process struct with one stereo f32 port
    // each way (audio-ports extension), valid for `frames_count` frames.
    unsafe {
        let s = state(p);
        let pr = &*process;
        if pr.audio_inputs_count < 1 || pr.audio_outputs_count < 1 {
            return CLAP_PROCESS_ERROR;
        }
        let input: &clap_audio_buffer = &*pr.audio_inputs;
        let output: &mut clap_audio_buffer = &mut *pr.audio_outputs;
        if input.data32.is_null() || output.data32.is_null() || input.channel_count < 2 {
            return CLAP_PROCESS_ERROR;
        }
        if output.channel_count < 2 {
            return CLAP_PROCESS_ERROR;
        }
        let (il, ir) = (*input.data32, *input.data32.add(1));
        let (ol, or) = (*output.data32, *output.data32.add(1));
        let frames = pr.frames_count as usize;
        let events = &*pr.in_events;
        let count = events.size.map_or(0, |f| f(events));
        let mut pos = 0usize;
        let run = |s: &State, from: usize, to: usize| {
            for k in from..to {
                *ol.add(k) = *il.add(k) * s.gain;
                *or.add(k) = *ir.add(k) * s.gain;
            }
        };
        for i in 0..count {
            let Some(get) = events.get else { break };
            let h = get(events, i);
            if h.is_null() {
                continue;
            }
            let at = ((*h).time as usize).clamp(pos, frames);
            run(s, pos, at);
            pos = at;
            apply_event(s, h, pr.out_events);
        }
        run(s, pos, frames);
        if s.resets != s.resets_reported {
            s.resets_reported = s.resets;
            push_param(pr.out_events, 0, PARAM_RESETS, f64::from(s.resets));
        }
    }
    CLAP_PROCESS_CONTINUE
}

// ---- audio-ports ----

unsafe extern "C" fn ports_count(_: *const clap_plugin, _is_input: bool) -> u32 {
    1
}

unsafe extern "C" fn ports_get(
    _: *const clap_plugin,
    index: u32,
    _is_input: bool,
    info: *mut clap_audio_port_info,
) -> bool {
    if index != 0 {
        return false;
    }
    // SAFETY: `info` is a valid out pointer.
    unsafe {
        let info = &mut *info;
        info.id = 0;
        info.name = [0; 256];
        for (dst, src) in info.name.iter_mut().zip(c"main".to_bytes()) {
            *dst = *src as c_char;
        }
        info.flags = CLAP_AUDIO_PORT_IS_MAIN;
        info.channel_count = 2;
        info.port_type = CLAP_PORT_STEREO.as_ptr();
        info.in_place_pair = CLAP_INVALID_ID;
    }
    true
}

static AUDIO_PORTS: clap_plugin_audio_ports = clap_plugin_audio_ports {
    count: Some(ports_count),
    get: Some(ports_get),
};

// ---- params ----

unsafe extern "C" fn params_count(_: *const clap_plugin) -> u32 {
    3
}

unsafe extern "C" fn params_get_info(
    _: *const clap_plugin,
    index: u32,
    info: *mut clap_param_info,
) -> bool {
    let (id, name, flags, min, max, default) = match index {
        0 => (
            PARAM_GAIN,
            c"Gain",
            CLAP_PARAM_IS_AUTOMATABLE,
            0.0,
            2.0,
            1.0,
        ),
        1 => (
            PARAM_MODE,
            c"Mode",
            CLAP_PARAM_IS_AUTOMATABLE | CLAP_PARAM_IS_STEPPED,
            0.0,
            3.0,
            0.0,
        ),
        2 => (
            PARAM_RESETS,
            c"Resets",
            CLAP_PARAM_IS_READONLY | CLAP_PARAM_IS_HIDDEN,
            0.0,
            1000.0,
            0.0,
        ),
        _ => return false,
    };
    // SAFETY: `info` is a valid out pointer.
    unsafe {
        let info = &mut *info;
        info.id = id;
        info.flags = flags;
        info.cookie = ptr::null_mut();
        info.name = [0; 256];
        for (dst, src) in info.name.iter_mut().zip(name.to_bytes()) {
            *dst = *src as c_char;
        }
        info.module = [0; 1024];
        info.min_value = min;
        info.max_value = max;
        info.default_value = default;
    }
    true
}

unsafe extern "C" fn params_get_value(p: *const clap_plugin, id: u32, out: *mut f64) -> bool {
    // SAFETY: valid plugin and out pointers.
    unsafe {
        let s = state(p);
        *out = match id {
            PARAM_GAIN => f64::from(s.gain),
            PARAM_MODE => f64::from(s.mode),
            PARAM_RESETS => f64::from(s.resets),
            _ => return false,
        };
    }
    true
}

unsafe extern "C" fn params_flush(
    p: *const clap_plugin,
    input: *const clap_input_events,
    output: *const clap_output_events,
) {
    // SAFETY: valid plugin and event lists.
    unsafe {
        let s = state(p);
        let input = &*input;
        let count = input.size.map_or(0, |f| f(input));
        for i in 0..count {
            if let Some(get) = input.get {
                let h = get(input, i);
                if !h.is_null() {
                    apply_event(s, h, output);
                }
            }
        }
    }
}

static PARAMS: clap_plugin_params = clap_plugin_params {
    count: Some(params_count),
    get_info: Some(params_get_info),
    get_value: Some(params_get_value),
    value_to_text: None,
    text_to_value: None,
    flush: Some(params_flush),
};

// ---- latency, tail ----

unsafe extern "C" fn latency_get(_: *const clap_plugin) -> u32 {
    LATENCY_FRAMES
}

static LATENCY: clap_plugin_latency = clap_plugin_latency {
    get: Some(latency_get),
};

unsafe extern "C" fn tail_get(_: *const clap_plugin) -> u32 {
    TAIL_FRAMES
}

static TAIL: clap_plugin_tail = clap_plugin_tail {
    get: Some(tail_get),
};

// ---- state ----

unsafe extern "C" fn state_save(p: *const clap_plugin, stream: *const clap_ostream) -> bool {
    // SAFETY: valid plugin and stream.
    unsafe {
        let s = state(p);
        let mut bytes = [0u8; 8];
        bytes[..4].copy_from_slice(&s.gain.to_le_bytes());
        bytes[4..].copy_from_slice(&s.mode.to_le_bytes());
        let Some(write) = (*stream).write else {
            return false;
        };
        let mut done = 0usize;
        while done < bytes.len() {
            let n = write(
                stream,
                bytes.as_ptr().add(done).cast::<c_void>(),
                (bytes.len() - done) as u64,
            );
            if n <= 0 {
                return false;
            }
            done += n as usize;
        }
    }
    true
}

unsafe extern "C" fn state_load(p: *const clap_plugin, stream: *const clap_istream) -> bool {
    // SAFETY: valid plugin and stream.
    unsafe {
        let s = state(p);
        let mut bytes = [0u8; 8];
        let Some(read) = (*stream).read else {
            return false;
        };
        let mut done = 0usize;
        while done < bytes.len() {
            let n = read(
                stream,
                bytes.as_mut_ptr().add(done).cast::<c_void>(),
                (bytes.len() - done) as u64,
            );
            if n <= 0 {
                return false;
            }
            done += n as usize;
        }
        s.gain = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        s.mode = f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    }
    true
}

static STATE: clap_plugin_state = clap_plugin_state {
    save: Some(state_save),
    load: Some(state_load),
};

unsafe extern "C" fn plugin_get_extension(
    _: *const clap_plugin,
    id: *const c_char,
) -> *const c_void {
    // SAFETY: the host passes a NUL-terminated id.
    let id = unsafe { CStr::from_ptr(id) };
    if id == CLAP_EXT_AUDIO_PORTS {
        ptr::from_ref(&AUDIO_PORTS).cast()
    } else if id == CLAP_EXT_PARAMS {
        ptr::from_ref(&PARAMS).cast()
    } else if id == CLAP_EXT_LATENCY {
        ptr::from_ref(&LATENCY).cast()
    } else if id == CLAP_EXT_TAIL {
        ptr::from_ref(&TAIL).cast()
    } else if id == CLAP_EXT_STATE {
        ptr::from_ref(&STATE).cast()
    } else {
        ptr::null()
    }
}

// ---- factory and entry ----

unsafe extern "C" fn factory_count(_: *const clap_plugin_factory) -> u32 {
    1
}

unsafe extern "C" fn factory_descriptor(
    _: *const clap_plugin_factory,
    index: u32,
) -> *const clap_plugin_descriptor {
    if index == 0 {
        ptr::from_ref(current_descriptor())
    } else {
        ptr::null()
    }
}

unsafe extern "C" fn factory_create(
    _: *const clap_plugin_factory,
    _host: *const clap_host,
    plugin_id: *const c_char,
) -> *const clap_plugin {
    let desc = current_descriptor();
    // SAFETY: NUL-terminated strings from the host and from our statics.
    if unsafe { CStr::from_ptr(plugin_id) != CStr::from_ptr(desc.id) } {
        return ptr::null();
    }
    let mut inst = Box::new(Instance {
        raw: clap_plugin {
            desc,
            plugin_data: ptr::null_mut(),
            init: Some(plugin_init),
            destroy: Some(plugin_destroy),
            activate: Some(plugin_activate),
            deactivate: Some(plugin_deactivate),
            start_processing: Some(plugin_start),
            stop_processing: Some(plugin_stop),
            reset: Some(plugin_reset),
            process: Some(plugin_process),
            get_extension: Some(plugin_get_extension),
            on_main_thread: Some(plugin_on_main_thread),
        },
        state: State {
            gain: 1.0,
            mode: 0.0,
            resets: 0,
            resets_reported: 0,
        },
    });
    inst.raw.plugin_data = ptr::from_mut(&mut inst.state).cast();
    // `raw` is the first field of a `repr(C)` struct, so this is also the
    // `clap_plugin` pointer.
    Box::into_raw(inst).cast::<clap_plugin>().cast_const()
}

static FACTORY: clap_plugin_factory = clap_plugin_factory {
    get_plugin_count: Some(factory_count),
    get_plugin_descriptor: Some(factory_descriptor),
    create_plugin: Some(factory_create),
};

unsafe extern "C" fn entry_init(path: *const c_char) -> bool {
    // SAFETY: the host passes a NUL-terminated path.
    let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
    let stem = std::path::Path::new(path.as_ref())
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mode = match stem {
        "crash" => MODE_CRASH,
        "hang" => MODE_HANG,
        _ => MODE_GAIN,
    };
    MODE.store(mode, Ordering::Relaxed);
    true
}

unsafe extern "C" fn entry_deinit() {}

unsafe extern "C" fn entry_get_factory(id: *const c_char) -> *const c_void {
    // SAFETY: the host passes a NUL-terminated id.
    if unsafe { CStr::from_ptr(id) } == CLAP_PLUGIN_FACTORY_ID {
        ptr::from_ref(&FACTORY).cast()
    } else {
        ptr::null()
    }
}

/// The CLAP entry point symbol.
#[repr(transparent)]
pub struct Entry(clap_plugin_entry);
// SAFETY: plain function pointers and a version struct.
unsafe impl Sync for Entry {}

#[unsafe(no_mangle)]
pub static clap_entry: Entry = Entry(clap_plugin_entry {
    clap_version: CLAP_VERSION,
    init: Some(entry_init),
    deinit: Some(entry_deinit),
    get_factory: Some(entry_get_factory),
});
