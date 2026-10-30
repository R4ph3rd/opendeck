//! Heap use on the audio path, seen from the C library.
//!
//! `assert_no_alloc` only watches Rust's global allocator. The Signalsmith
//! stretcher is C++: its `new`, `std::vector` growth and `free` go straight to
//! `malloc`, invisible to it. This module replaces `malloc` and friends in the
//! test binary with counting wrappers around glibc's own entry points, armed
//! per thread, so a test can assert that a stretch of code made no heap calls
//! at all, C++ included.
//!
//! Limits, stated plainly:
//!
//! - glibc on Linux only (the Ubuntu CI jobs). Windows, the product's main
//!   target, and macOS are not covered: the MSVC and Apple allocators have no
//!   equivalent hook here.
//! - It sees calls, not intent: a test only proves the inputs it feeds.
//!   Capacity growth that needs a rare spectrum (see
//!   `transposed_stretcher_makes_no_heap_calls`) is only caught if the input
//!   provokes it.
//! - Allocation inside the C++ standard library or the stretcher that never
//!   goes through `malloc` (a custom arena) would be missed. None is used.

#![allow(unsafe_code)]

use std::cell::Cell;
use std::ffi::{c_int, c_void};

use opendeck_contracts::tempo::TempoRange;
use opendeck_contracts::{DeckId, EngineCommand, Frames, SampleRate};
use signalsmith_stretch::Stretch;

use crate::{EngineConfig, Interpolation, OfflineRenderer, TrackBuffer};

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<u64> = const { Cell::new(0) };
}

fn note() {
    // `try_with`: a thread that is shutting down must not panic in malloc.
    let _ = ARMED.try_with(|armed| {
        if armed.get() {
            let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
        }
    });
}

unsafe extern "C" {
    fn __libc_malloc(size: usize) -> *mut c_void;
    fn __libc_free(ptr: *mut c_void);
    fn __libc_calloc(count: usize, size: usize) -> *mut c_void;
    fn __libc_realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn __libc_memalign(align: usize, size: usize) -> *mut c_void;
}

const ENOMEM: c_int = 12;

// SAFETY (all hooks): these forward to glibc's implementations with the same
// arguments, so they keep malloc's contract. They only bump a thread-local
// counter, which never allocates (const-initialised, no destructor).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    note();
    unsafe { __libc_malloc(size) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if !ptr.is_null() {
        note();
    }
    unsafe { __libc_free(ptr) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
    note();
    unsafe { __libc_calloc(count, size) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    note();
    unsafe { __libc_realloc(ptr, size) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn memalign(align: usize, size: usize) -> *mut c_void {
    note();
    unsafe { __libc_memalign(align, size) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn aligned_alloc(align: usize, size: usize) -> *mut c_void {
    note();
    unsafe { __libc_memalign(align, size) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_memalign(out: *mut *mut c_void, align: usize, size: usize) -> c_int {
    note();
    let ptr = unsafe { __libc_memalign(align, size) };
    if ptr.is_null() {
        return ENOMEM;
    }
    unsafe { *out = ptr };
    0
}

/// Runs `f` and returns its result with the number of `malloc`-family and
/// `free` calls this thread made meanwhile.
fn count_heap_calls<R>(f: impl FnOnce() -> R) -> (R, u64) {
    CALLS.with(|c| c.set(0));
    ARMED.with(|a| a.set(true));
    let result = f();
    ARMED.with(|a| a.set(false));
    (result, CALLS.with(Cell::get))
}

const SR: u32 = 48_000;

fn deck0() -> DeckId {
    DeckId::new(0).expect("deck 0 exists")
}

/// Deterministic noise in `[-0.5, 0.5]`, interleaved stereo.
fn noise(frames: usize) -> Vec<f32> {
    let mut seed = 0x1234_5678_u32;
    (0..frames * 2)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed >> 8) as f32 / (1 << 24) as f32 - 0.5
        })
        .collect()
}

#[test]
fn the_counter_sees_rust_and_c_plus_plus_allocations() {
    let (_, rust) = count_heap_calls(|| std::hint::black_box(vec![1_u8; 4096]));
    assert!(rust >= 1, "a Rust Vec must register ({rust})");
    // Building a stretcher runs C++ `new` and `std::vector` code only.
    let (stretch, cpp) = count_heap_calls(|| Stretch::preset_default(2, SR));
    assert!(cpp >= 1, "constructing a stretcher must register ({cpp})");
    drop(stretch);
}

/// Every key lock path of a deck: starts, seeks, stops, rate changes, bend,
/// key lock toggles, odd block sizes. Zero `malloc` or `free` calls from
/// either language.
#[test]
fn key_lock_playback_makes_no_heap_calls() {
    let config = EngineConfig {
        sample_rate: SampleRate(SR),
        max_block_frames: 512,
        deck_count: 2,
        interpolation: Interpolation::Cubic,
    };
    let mut r = OfflineRenderer::new(config)
        .expect("valid config")
        .with_block_frames(333);
    let track = TrackBuffer::from_vec(noise(SR as usize * 6)).expect("track");
    r.handle()
        .load_track(deck0(), track, None)
        .expect("hand-off ring has room");
    let mut buf = vec![0.0; 1024 * 2];
    r.render_into(&mut buf);

    let mut seed = 0x9E37_79B9_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut commands = Vec::new();
    for _ in 0..300 {
        commands.push(match next() % 7 {
            0 => EngineCommand::SetPlaying {
                deck: deck0(),
                playing: next() % 3 != 0,
            },
            1 => EngineCommand::Seek {
                deck: deck0(),
                position: Frames(u64::from(next() % 200_000)),
            },
            2 => EngineCommand::SetKeyLock {
                deck: deck0(),
                on: next() % 4 != 0,
            },
            3 => EngineCommand::SetTempoRange {
                deck: deck0(),
                range: TempoRange::CYCLE[(next() % 4) as usize],
            },
            4 => EngineCommand::SetTempoFader {
                deck: deck0(),
                position: (next() % 2001) as f32 / 1000.0 - 1.0,
            },
            5 => EngineCommand::PitchBend {
                deck: deck0(),
                amount: (next() % 201) as f32 / 100.0 - 1.0,
            },
            _ => EngineCommand::Cue {
                deck: deck0(),
                pressed: next() % 2 == 0,
            },
        });
    }
    r.handle()
        .send(EngineCommand::SetKeyLock {
            deck: deck0(),
            on: true,
        })
        .expect("command ring has room");
    r.handle()
        .send(EngineCommand::SetPlaying {
            deck: deck0(),
            playing: true,
        })
        .expect("command ring has room");

    let mut blocks = 0;
    let mut audible = 0;
    let mut heap_calls = 0;
    for command in commands {
        r.handle().send(command).expect("command ring has room");
        let (_, calls) = count_heap_calls(|| r.render_into(&mut buf));
        heap_calls += calls;
        blocks += 1;
        audible += usize::from(buf.iter().any(|s| s.abs() > 0.01));
        r.handle().reclaim();
        r.handle().poll_telemetry();
        while r.handle().poll_event().is_some() {}
    }
    assert!(
        audible > blocks / 4,
        "only {audible} of {blocks} blocks played"
    );
    assert_eq!(heap_calls, 0, "{heap_calls} heap calls in {blocks} blocks");
}

/// Two equal impulses half an analysis window apart make every other FFT bin
/// a local maximum: the densest spectrum the peak finder can see.
fn comb_signal(frames: usize, period: usize) -> Vec<f32> {
    let mut data = vec![0.0; frames * 2];
    for frame in (0..frames).step_by(period) {
        if let Some(pair) = data.get_mut(frame * 2..frame * 2 + 2) {
            pair.fill(0.8);
        }
    }
    data
}

/// The engine does not transpose today, so the stretcher never runs its peak
/// finder (`findPeaks`, only called when the frequency map is not the
/// identity). Key shift (M7) will. Its peak list is reserved for `bands / 2`
/// entries and peaks are at least two bands apart, so it overflows only when
/// `bands` is odd. It never is: `bands = fftSamples / 2` and
/// `fftSamples = 2 * RealFFT::fastSizeAbove(..)`, a multiple of 4
/// (signalsmith-linear `stft.h` `configure`, `fft.h` `fastSizeAbove`). No
/// public API reads `bands`, so this cannot be asserted directly; instead
/// feed the densest spectrum with transposition and formant shifting on and
/// require zero heap calls for any buffer.
#[test]
fn transposed_stretcher_makes_no_heap_calls() {
    let noise = noise(SR as usize);
    for (name, input) in [
        ("comb 2880", comb_signal(SR as usize, 2880)),
        ("comb 1440", comb_signal(SR as usize, 1440)),
        ("comb 3001", comb_signal(SR as usize, 3001)),
        ("noise", noise),
    ] {
        for semitones in [-5.0, 3.0, 7.0] {
            let mut stretch = Stretch::preset_default(2, SR);
            stretch.set_transpose_factor_semitones(semitones, None);
            stretch.set_formant_factor_semitones(1.0, true);
            let mut out = vec![0.0; 512 * 2];
            let latency = stretch.input_latency();
            stretch.seek(&input[..latency * 2], 1.0);
            let (_, calls) = count_heap_calls(|| {
                for chunk in input.chunks(512 * 2) {
                    stretch.process(chunk, &mut out[..chunk.len()]);
                }
            });
            assert_eq!(calls, 0, "{name} at {semitones} semitones: {calls} calls");
        }
    }
}

/// The FX host on every path: inserts, sends, master, enable and disable
/// (tails included), mix, beat division, parameters, effect swaps and
/// clears. The control side builds and drops effects outside the counted
/// region; the callback itself makes zero `malloc` or `free` calls.
#[test]
fn fx_host_makes_no_heap_calls() {
    use crate::FxTempoSource;
    use crate::tests_fx::{Kind, fx, gain_fx};
    use opendeck_contracts::fx::ParamId;
    use opendeck_contracts::{Beats, Bpm, FxSlotId, FxUnit, Gain};

    let config = EngineConfig {
        sample_rate: SampleRate(SR),
        max_block_frames: 512,
        deck_count: 2,
        interpolation: Interpolation::Cubic,
    };
    let mut r = OfflineRenderer::new(config)
        .expect("valid config")
        .with_block_frames(333);
    for d in 0..2u8 {
        let track = TrackBuffer::from_vec(noise(SR as usize * 6)).expect("track");
        r.handle()
            .load_track(DeckId::new(d).expect("deck"), track, None)
            .expect("hand-off ring has room");
    }
    let units = [
        FxUnit::Deck(deck0()),
        FxUnit::Deck(DeckId::new(1).expect("deck 1")),
        FxUnit::Master,
        FxUnit::Send(0),
        FxUnit::Send(1),
    ];
    let slot = |unit: FxUnit, i: usize| FxSlotId::new(unit, i as u8).expect("slot");
    let kinds = [Kind::Gain, Kind::Echo, Kind::Latency(64)];
    for unit in units {
        for (i, kind) in kinds.into_iter().enumerate() {
            r.handle()
                .install_effect(slot(unit, i), fx(kind).0)
                .map_err(|_| ())
                .expect("ring");
            r.handle()
                .send(EngineCommand::SetFxEnabled {
                    slot: slot(unit, i),
                    on: true,
                })
                .expect("ring");
            r.render_into(&mut vec![0.0; 1024]);
        }
    }
    for d in 0..2u8 {
        let deck = DeckId::new(d).expect("deck");
        r.handle()
            .send(EngineCommand::SetPlaying {
                deck,
                playing: true,
            })
            .expect("ring");
        for bus in 0..2 {
            r.handle()
                .send(EngineCommand::SetFxSend {
                    deck,
                    bus,
                    level: Gain(0.7),
                })
                .expect("ring");
        }
    }
    let mut buf = vec![0.0; 1000 * 2];
    r.render_into(&mut buf);

    let mut seed = 0xC0FF_EE11_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut heap_calls = 0;
    let mut blocks = 0;
    let mut audible = 0;
    for _ in 0..400 {
        let unit = units[(next() % 5) as usize];
        let id = slot(unit, (next() % 3) as usize);
        let command = match next() % 6 {
            0 => EngineCommand::SetFxEnabled {
                slot: id,
                on: next() % 2 == 0,
            },
            1 => EngineCommand::SetFxMix {
                slot: id,
                wet: (next() % 1001) as f32 / 1000.0,
            },
            2 => EngineCommand::SetFxParam {
                slot: id,
                param: ParamId(next() % 2),
                value: (next() % 1201) as f32 / 1000.0 - 0.1,
            },
            3 => EngineCommand::SetFxBeatDivision {
                slot: id,
                beats: Beats(f64::from(next() % 40) / 8.0),
            },
            4 => EngineCommand::SetFxSend {
                deck: DeckId::new((next() % 2) as u8).expect("deck"),
                bus: (next() % 2) as u8,
                level: Gain((next() % 1001) as f32 / 500.0),
            },
            _ => EngineCommand::SetFxEnabled { slot: id, on: true },
        };
        r.handle().send(command).expect("ring");
        match next() % 9 {
            0 => {
                let kind = kinds[(next() % 3) as usize];
                let effect = if matches!(kind, Kind::Gain) {
                    gain_fx(0.8).0
                } else {
                    fx(kind).0
                };
                let _ = r.handle().install_effect(id, effect);
            }
            1 => {
                let _ = r.handle().clear_effect(id);
            }
            2 => {
                let at = Frames(u64::from(next() % 400_000));
                let _ = r
                    .handle()
                    .schedule_fx_enabled(id, next() % 2 == 0, at, next() % 2 == 0);
            }
            3 => {
                let source = match next() % 3 {
                    0 => FxTempoSource::Auto,
                    1 => FxTempoSource::Deck(DeckId::new((next() % 2) as u8).expect("deck")),
                    _ => FxTempoSource::Tap(Bpm(60.0 + f64::from(next() % 100))),
                };
                let _ = r.handle().set_fx_tempo_source(unit, source);
            }
            4 => {
                let to = slot(unit, (next() % 3) as usize);
                let at = Frames(u64::from(next() % 400_000));
                let _ = r.handle().release_fx(id, to, at, Frames(2400));
            }
            _ => {}
        }
        let (_, calls) = count_heap_calls(|| r.render_into(&mut buf));
        heap_calls += calls;
        blocks += 1;
        audible += usize::from(buf.iter().any(|s| s.abs() > 0.01));
        r.handle().reclaim();
        r.handle().poll_telemetry();
    }
    assert!(
        audible > blocks / 2,
        "only {audible} of {blocks} blocks played"
    );
    assert_eq!(heap_calls, 0, "{heap_calls} heap calls in {blocks} blocks");
}
