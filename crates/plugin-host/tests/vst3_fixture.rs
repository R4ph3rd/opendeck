//! The VST3 backend against the in-repo fixture plugin
//! (`tests/fixtures/vst3-test-plugin`, built with the `vst3` crate only):
//! scanning in a subprocess, blacklisting, bus setup, and the effect adapter,
//! for both a separate-controller and a single-component class.
//!
//! No third-party VST3 plugin is involved. A real plugin is covered by the
//! `#[ignore]` test at the end (`OPENDECK_TEST_VST3`).
#![cfg(feature = "vst3")]

mod vst3_support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::{EffectRegistry, ParamCurve, ParamId, StereoBufferMut};
use opendeck_contracts::plugin::{PluginKind, PluginScanStatus};
use opendeck_plugin_host::backend::{ParamChange, PluginInstance, WakeFlag};
use opendeck_plugin_host::main_thread::ThreadMainThread;
use opendeck_plugin_host::scan::ScanRecord;
use opendeck_plugin_host::scan_cache::{FormatFolders, RescanRequest, ScanCache};
use opendeck_plugin_host::vst3::{Vst3Backend, Vst3Instance};
use opendeck_plugin_host::{InstanceStatus, PluginHost};
use vst3_support::*;

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Runs `f` for the separate-controller and the single-component class.
fn both(f: impl Fn(&str)) {
    for id in [GAIN_ID, SINGLE_ID] {
        f(id);
    }
}

fn prepared(
    id: &str,
    frames: usize,
) -> (
    Box<dyn opendeck_contracts::fx::Effect>,
    opendeck_plugin_host::InstanceHandle,
) {
    prepared_on(&host(), id, frames)
}

// ---- scanning ----

#[test]
fn scan_lists_the_audio_classes_with_hex_ids() {
    let plugins = scanned();
    // The controller class is not a plugin: three audio classes remain.
    assert_eq!(plugins.len(), 3);
    let gain = info_of(GAIN_ID);
    assert_eq!(gain.key, key(GAIN_ID));
    assert_eq!(gain.kind, PluginKind::Effect);
    assert_eq!(gain.name, "Test Gain");
    assert_eq!(gain.vendor, "OpenDeck tests");
    assert_eq!(gain.version, "1.2.3");
    assert_eq!(gain.categories, ["Fx", "Dynamics"]);
    assert_eq!(gain.path, gain_bundle().display().to_string());
    assert_eq!(info_of(SINGLE_ID).name, "Test Single Gain");
}

#[test]
fn subprocess_scan_validates_and_refuses_the_mono_plugin() {
    let record = scanner(Duration::from_secs(20)).scan("vst3", &gain_bundle());
    assert_eq!(record.status, PluginScanStatus::Ok);
    let ids: Vec<_> = record.plugins.iter().map(|p| p.key.id.as_str()).collect();
    assert_eq!(ids, [GAIN_ID, SINGLE_ID], "mono class failed validation");
    assert_eq!(record.params.len(), 2);
    assert!(record.params.iter().all(|p| p.params.len() == 2));
    assert!(record.is_current());
    assert!(record.size > 0, "stamped by the library inside the bundle");
}

#[test]
fn crashing_plugin_is_blacklisted() {
    let record = scanner(Duration::from_secs(20)).scan("vst3", &fixture_dir().join("crash.vst3"));
    assert_eq!(record.status, PluginScanStatus::Crashed);
    assert!(record.status.blacklists());
    assert!(record.plugins.is_empty());
}

#[test]
fn hanging_plugin_times_out_and_is_blacklisted() {
    let started = std::time::Instant::now();
    let record = scanner(Duration::from_secs(2)).scan("vst3", &fixture_dir().join("hang.vst3"));
    assert_eq!(record.status, PluginScanStatus::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_missing_library_is_a_failed_scan_not_a_blacklist() {
    let empty = std::env::temp_dir().join(format!("odph-empty-{}.vst3", std::process::id()));
    std::fs::create_dir_all(&empty).expect("mkdir");
    let record = scanner(Duration::from_secs(20)).scan("vst3", &empty);
    assert!(
        matches!(record.status, PluginScanStatus::Failed { .. }),
        "{record:?}"
    );
    assert!(!record.status.blacklists());
    let _ = std::fs::remove_dir_all(&empty);
}

#[test]
fn scan_cache_reuses_blacklists_and_rescans_a_replaced_library() {
    let dir = std::env::temp_dir().join(format!("odph-vst3-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for name in ["gain", "crash", "hang"] {
        copy_bundle(
            &binary_of(&fixture_dir().join(format!("{name}.vst3"))),
            &dir.join(format!("{name}.vst3")),
        );
    }
    let formats = [FormatFolders {
        provider: "vst3".into(),
        extension: "vst3".into(),
        folders: vec![dir.clone()],
    }];
    let scanner = scanner(Duration::from_secs(3));
    let mut cache = ScanCache::new();

    let first = cache.rescan(&scanner, &formats, &RescanRequest::default(), |_| {});
    assert_eq!((first.found, first.scanned, first.reused), (3, 3, 0));
    assert_eq!(cache.catalog().len(), 2);
    assert_eq!(cache.blacklist().count(), 2);

    let json = serde_json::to_string(&cache).expect("serialize");
    let mut cache: ScanCache = serde_json::from_str(&json).expect("deserialize");
    let second = cache.rescan(&scanner, &formats, &RescanRequest::default(), |_| {});
    assert_eq!((second.scanned, second.reused), (0, 3));

    // Replacing the library inside a bundle changes the stamp, although the
    // bundle directory itself is untouched.
    let lib = binary_of(&dir.join("gain.vst3"));
    let mut bytes = std::fs::read(&lib).expect("read");
    bytes.extend_from_slice(b"padding");
    std::fs::write(&lib, bytes).expect("write");
    let third = cache.rescan(&scanner, &formats, &RescanRequest::default(), |_| {});
    assert_eq!((third.scanned, third.reused), (1, 2));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- host pieces without the adapter ----

#[test]
fn instances_report_parameters_latency_tail_and_plugin_text() {
    both(|id| {
        let mut inst =
            Vst3Instance::create(&info_of(id), Arc::new(WakeFlag::default())).expect("create");
        assert_eq!(inst.has_separate_controller(), id == GAIN_ID);
        let params = inst.params();
        // The hidden read-only "Resets" parameter is not listed.
        assert_eq!(params.len(), 2);
        let gain = &params[0];
        assert_eq!((gain.id, gain.name.as_str()), (ParamId(0), "Gain"));
        assert_eq!((gain.min, gain.max, gain.default), (0.0, 1.0, 0.5));
        assert_eq!(gain.curve, ParamCurve::Linear);
        let mode = &params[1];
        assert_eq!((mode.id, mode.name.as_str()), (ParamId(1), "Mode"));
        assert_eq!(mode.curve, ParamCurve::Stepped { steps: 4 });
        assert!(params.iter().all(|p| p.is_valid()));

        // Latency and tail are known after activation only.
        assert_eq!((inst.latency_frames(), inst.tail_frames()), (0, None));
        let processor = inst.activate(SampleRate(48_000), 256).expect("activate");
        assert_eq!(inst.latency_frames(), LATENCY);
        assert_eq!(inst.tail_frames(), Some(TAIL));
        assert!(
            inst.activate(SampleRate(48_000), 256).is_err(),
            "already active"
        );
        inst.deactivate(processor);
        assert_eq!(inst.latency_frames(), 0);

        // Normalized <-> plain through the controller.
        assert_eq!(inst.normalized_to_plain(ParamId(0), 0.25), Some(0.5));
        assert_eq!(inst.plain_to_normalized(ParamId(0), 1.0), Some(0.5));
        assert_eq!(inst.value_text(ParamId(0), 0.5).as_deref(), Some("1.00 x"));
        assert_eq!(inst.value_text(ParamId(9), 0.5), None);
    });
}

#[test]
fn mono_plugins_are_refused_at_creation() {
    let info = {
        let mut i = info_of(GAIN_ID);
        i.key = key(MONO_ID);
        i
    };
    let err = Vst3Instance::create(&info, Arc::new(WakeFlag::default()))
        .err()
        .expect("mono refused");
    assert!(err.to_string().contains("stereo"), "{err}");
}

// ---- the effect adapter ----

#[test]
fn audio_is_processed_in_place_through_the_plugin() {
    both(|id| {
        let (mut fx, _handle) = prepared(id, 256);
        assert!(
            run(fx.as_mut(), 0.5, 256).iter().all(|s| *s == 0.5),
            "unity by default"
        );
        // Normalized 0.25 is a gain of 0.5, applied from the next block.
        fx.set_parameter(ParamId(0), 0.25);
        assert!(
            run(fx.as_mut(), 0.8, 256)
                .iter()
                .all(|s| (*s - 0.4).abs() < 1e-6)
        );
        // Smaller blocks than prepared work too.
        assert_eq!(run(fx.as_mut(), 1.0, 17), vec![0.5; 17]);
    });
}

#[test]
fn parameter_change_lands_on_its_frame_inside_the_block() {
    both(|id| {
        let (mut fx, handle) = prepared(id, 256);
        handle
            .send_param(ParamChange {
                id: ParamId(0),
                normalized: 0.25,
                offset: 100,
            })
            .expect("send");
        let out = run(fx.as_mut(), 1.0, 256);
        assert!(out[..100].iter().all(|s| *s == 1.0), "before the event");
        assert!(
            out[100..].iter().all(|s| (*s - 0.5).abs() < 1e-6),
            "from the event"
        );
        for (offset, normalized) in [(200, 1.0), (50, 0.0)] {
            handle
                .send_param(ParamChange {
                    id: ParamId(0),
                    normalized,
                    offset,
                })
                .expect("send");
        }
        let out = run(fx.as_mut(), 1.0, 256);
        assert!(out[..50].iter().all(|s| (*s - 0.5).abs() < 1e-6));
        assert!(out[50..200].iter().all(|s| *s == 0.0));
        assert!(out[200..].iter().all(|s| (*s - 2.0).abs() < 1e-6));
    });
}

#[test]
fn processing_does_not_allocate() {
    both(|id| {
        let (mut fx, handle) = prepared(id, 512);
        let mut l = vec![0.25_f32; 512];
        let mut r = vec![0.25_f32; 512];
        let c = ctx();
        for round in 0..4u32 {
            handle
                .send_param(ParamChange {
                    id: ParamId(0),
                    normalized: 0.5,
                    offset: 10 * round,
                })
                .expect("send");
            handle
                .send_param(ParamChange {
                    id: ParamId(1),
                    normalized: 1.0,
                    offset: 3,
                })
                .expect("send");
            assert_no_alloc::assert_no_alloc(|| {
                fx.set_parameter(ParamId(0), 0.75);
                fx.set_parameter(ParamId(7), 0.1);
                fx.set_parameter(ParamId(0), f32::NAN);
                let io = StereoBufferMut::new(&mut l, &mut r).expect("len");
                fx.process(io, &c);
                fx.reset();
                assert_eq!(fx.latency_frames(), LATENCY);
            });
            l.fill(0.25);
            r.fill(0.25);
        }
    });
}

#[test]
fn unknown_and_oversized_input_is_safe() {
    let (mut fx, _handle) = prepared(GAIN_ID, 64);
    fx.set_parameter(ParamId(999), 0.5);
    fx.set_parameter(ParamId(2), 0.5); // read-only, not settable
    assert_eq!(run(fx.as_mut(), 0.7, 64), vec![0.7; 64]);
    // A block larger than prepared passes the input through untouched.
    assert_eq!(run(fx.as_mut(), 0.7, 128), vec![0.7; 128]);
}

#[test]
fn plugin_output_changes_reach_the_main_thread() {
    let (mut fx, handle) = prepared(GAIN_ID, 64);
    handle
        .send_param(ParamChange {
            id: ParamId(1),
            normalized: 1.0,
            offset: 5,
        })
        .expect("send");
    run(fx.as_mut(), 0.0, 64);
    let out = handle.drain_param_output().expect("drain");
    assert_eq!(
        out,
        vec![ParamChange {
            id: ParamId(1),
            normalized: 1.0,
            offset: 5
        }]
    );
}

#[test]
fn reset_reaches_the_plugin() {
    let (mut fx, handle) = prepared(GAIN_ID, 64);
    run(fx.as_mut(), 0.0, 64);
    assert!(handle.drain_param_output().expect("drain").is_empty());
    fx.reset();
    fx.reset();
    run(fx.as_mut(), 0.0, 64);
    // The plugin counts `setProcessing(false)` calls and reports the count
    // as parameter 2 (hidden, read-only, range 0..1000) in the next block.
    let out = handle.drain_param_output().expect("drain");
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].id, ParamId(2));
    assert!((out[0].normalized - 0.002).abs() < 1e-6, "{out:?}");
}

#[test]
fn latency_and_tail_come_from_the_plugin() {
    let (fx, handle) = prepared(GAIN_ID, 64);
    assert_eq!(fx.latency_frames(), LATENCY);
    assert_eq!(handle.latency_frames(), Ok(LATENCY));
    assert_eq!(handle.tail_frames(), Ok(Some(TAIL)));
    assert!(fx.info().has_tail);
}

#[test]
fn latency_is_zero_before_prepare() {
    let host = host();
    let (fx, handle) = host.create_effect(&key(GAIN_ID)).expect("create");
    assert_eq!(fx.latency_frames(), 0);
    assert_eq!(handle.status(), Ok(InstanceStatus::Inactive));
}

#[test]
fn state_round_trips_between_instances() {
    both(|id| {
        let host = host();
        let (mut a, ha) = host.create_effect(&key(id)).expect("create a");
        a.prepare(SampleRate(48_000), 64);
        let default_state = ha.save_state().expect("save");
        ha.send_param(ParamChange {
            id: ParamId(0),
            normalized: 0.75,
            offset: 0,
        })
        .expect("send");
        // The mode goes to the controller too: it is part of the saved state.
        ha.send_param(ParamChange {
            id: ParamId(1),
            normalized: 1.0,
            offset: 0,
        })
        .expect("send");
        run(a.as_mut(), 0.0, 64);
        let saved = ha.save_state().expect("save");
        assert_ne!(saved, default_state);
        // OPV3, version, then two length-prefixed 8-byte parts: the
        // controller's part ends with the mode (f64 LE) the host mirrored.
        assert_eq!(&saved[..5], b"OPV3\x01");
        assert_eq!(saved.len(), 5 + 4 + 8 + 4 + 8);
        assert_eq!(&saved[saved.len() - 8..], &1.0f64.to_le_bytes());

        let (mut b, hb) = host.create_effect(&key(id)).expect("create b");
        hb.load_state(saved.clone()).expect("load");
        assert_eq!(hb.save_state().expect("save"), saved);
        b.prepare(SampleRate(44_100), 64);
        // Gain 0.75 is 1.5x, restored from the state, not the default.
        assert!(
            run(b.as_mut(), 1.0, 64)
                .iter()
                .all(|s| (*s - 1.5).abs() < 1e-6)
        );
        assert!(hb.load_state(vec![1, 2, 3]).is_err());
    });
}

#[test]
fn reprepare_at_another_rate_and_block_size_works() {
    let (mut fx, handle) = prepared(GAIN_ID, 64);
    fx.prepare(SampleRate(96_000), 1024);
    assert_eq!(handle.status(), Ok(InstanceStatus::Active));
    assert_eq!(fx.latency_frames(), LATENCY);
    assert_eq!(run(fx.as_mut(), 0.5, 1024), vec![0.5; 1024]);
}

#[test]
fn dropping_the_effect_destroys_the_instance() {
    let (fx, handle) = prepared(GAIN_ID, 64);
    drop(fx);
    assert!(matches!(
        handle.status(),
        Err(opendeck_plugin_host::PluginError::UnknownInstance(_))
    ));
}

#[test]
fn vst3_effects_register_under_vst3_keys() {
    let host = host();
    let mut registry = EffectRegistry::new();
    for factory in host.effect_factories() {
        registry.register(factory).expect("register");
    }
    let keys: Vec<_> = registry.effects().iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys, [key(GAIN_ID), key(SINGLE_ID), key(MONO_ID)]);
    assert!(
        registry.effects()[0].params.is_empty(),
        "filled by the scan"
    );
    let record = scanner(Duration::from_secs(20)).scan("vst3", &gain_bundle());
    host.set_param_table(record.params);
    assert_eq!(registry.effects()[0].params.len(), 2);
    let mut fx = registry.create(&key(GAIN_ID)).expect("create");
    fx.prepare(SampleRate(48_000), 32);
    assert_eq!(fx.info().params.len(), 2);
    assert!(
        registry
            .create(&key("00000000000000000000000000000000"))
            .is_err()
    );
    assert!(registry.create(&key("nothex")).is_err());
}

/// With a real plugin: `OPENDECK_TEST_VST3=/path/Effect.vst3 cargo test -p
/// opendeck-plugin-host -- --ignored real_vst3`. Not run in CI.
#[test]
#[ignore = "needs a real VST3 effect; set OPENDECK_TEST_VST3"]
fn real_vst3_effect_scans_and_processes() {
    let path = PathBuf::from(std::env::var("OPENDECK_TEST_VST3").expect("OPENDECK_TEST_VST3"));
    let record: ScanRecord = scanner(Duration::from_secs(30)).scan("vst3", &path);
    assert_eq!(record.status, PluginScanStatus::Ok, "{record:?}");
    let info = record
        .plugins
        .iter()
        .find(|p| p.kind == PluginKind::Effect)
        .expect("an effect in the bundle")
        .clone();
    let main = Arc::new(ThreadMainThread::new("plugin-main").expect("thread"));
    let host = PluginHost::new(main, vec![Arc::new(Vst3Backend)]);
    host.set_catalog(vec![info.clone()]);
    let (mut fx, handle) = host.create_effect(&info.key).expect("create");
    fx.prepare(SampleRate(48_000), 512);
    let out = run(fx.as_mut(), 0.1, 512);
    assert!(out.iter().all(|s| s.is_finite()));
    println!(
        "{}: {} params, latency {} frames, tail {:?}, state {} bytes",
        info.name,
        fx.info().params.len(),
        fx.latency_frames(),
        handle.tail_frames(),
        handle.save_state().map_or(0, |s| s.len())
    );
    let state = handle.save_state().expect("save");
    handle.load_state(state).expect("load");
}
