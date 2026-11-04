//! Editor edits and restart requests: the fixture's controller acts like a
//! plugin editor (`opendeck_test_simulate_edit`) and the host must route the
//! edit to the audio half and report the restart flags.
//!
//! Own test binary: the fixture remembers the last controller it created, so
//! no other test may create instances in this process.
#![cfg(feature = "vst3")]
// Loads the fixture library to call its exported edit hook.
#![allow(unsafe_code)]

mod vst3_support;

use std::time::{Duration, Instant};

use opendeck_plugin_host::InstanceHandle;
use vst3_support::*;

fn simulate_edit(id: u32, value: f64) {
    let lib = binary_of(&gain_bundle());
    // SAFETY: the same library the host loaded (the loader returns the
    // already loaded copy); the symbol has this signature.
    unsafe {
        let lib = libloading::Library::new(lib).expect("open fixture");
        let f = lib
            .get::<extern "C" fn(u32, f64)>(b"opendeck_test_simulate_edit\0")
            .expect("symbol");
        f(id, value);
    }
}

/// The wake flag is polled every 20 ms; give it a second.
fn requests_within(
    handle: &InstanceHandle,
    ok: impl Fn(&opendeck_plugin_host::backend::InstanceRequests) -> bool,
) -> opendeck_plugin_host::backend::InstanceRequests {
    let end = Instant::now() + Duration::from_secs(1);
    let mut seen = opendeck_plugin_host::backend::InstanceRequests::default();
    while Instant::now() < end {
        seen = seen.merge(handle.take_requests().expect("requests"));
        if ok(&seen) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    seen
}

#[test]
fn editor_edits_reach_the_processor_and_restart_flags_the_host() {
    for id in [GAIN_ID, SINGLE_ID] {
        let host = host();
        let (mut fx, handle) = prepared_on(&host, id, 128);
        assert!(run(fx.as_mut(), 1.0, 128).iter().all(|s| *s == 1.0));

        simulate_edit(0, 0.25);
        // The edit is applied from the next block: gain 0.5.
        assert!(
            run(fx.as_mut(), 1.0, 128)
                .iter()
                .all(|s| (*s - 0.5).abs() < 1e-6),
            "{id}"
        );

        let seen = requests_within(&handle, |r| r.latency_changed && r.state_dirty);
        assert!(
            seen.latency_changed,
            "kLatencyChanged -> latency_changed ({id})"
        );
        assert!(seen.state_dirty, "performEdit -> state_dirty ({id})");
        assert!(!seen.restart);
    }
}
