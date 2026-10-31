//! The CLAP backend against the in-repo fixture plugins
//! (`tests/fixtures/clap-test-plugin`, built with `clap-sys` only): scanning
//! in a subprocess, blacklisting, and the effect adapter.
//!
//! No third-party CLAP plugin is involved. A real plugin is covered by the
//! `#[ignore]` test at the end (`OPENDECK_TEST_CLAP`).
#![cfg(feature = "clap")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use opendeck_contracts::fx::{
    EffectKey, EffectRegistry, ParamCurve, ParamId, ProcessContext, StereoBufferMut,
};
use opendeck_contracts::plugin::{PluginInfo, PluginKind, PluginScanStatus};
use opendeck_contracts::{Beats, SampleRate};
use opendeck_plugin_host::backend::{ParamChange, PluginBackend};
use opendeck_plugin_host::clap::ClapBackend;
use opendeck_plugin_host::main_thread::ThreadMainThread;
use opendeck_plugin_host::scan::{ScanRecord, Scanner};
use opendeck_plugin_host::scan_cache::{FormatFolders, RescanRequest, ScanCache};
use opendeck_plugin_host::{InstanceHandle, InstanceStatus, PluginHost};

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const GAIN_ID: &str = "dev.opendeck.test.gain";
const LATENCY: u32 = 64;
const TAIL: u32 = 4800;

/// Builds the fixture cdylib (a no-op when cargo already built it) and
/// copies it to `gain.clap`, `crash.clap` and `hang.clap` in a temp folder.
fn fixture_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let exe = std::env::current_exe().expect("test exe");
        // target/<profile>/deps/<test exe>
        let profile_dir = exe.parent().and_then(Path::parent).expect("profile dir");
        let target_dir = profile_dir.parent().expect("target dir");
        let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cmd.args(["build", "-p", "opendeck-clap-test-plugin", "--target-dir"])
            .arg(target_dir);
        if profile_dir.file_name().is_some_and(|n| n == "release") {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("run cargo");
        assert!(status.success(), "fixture plugin build failed");
        let lib = [
            "libopendeck_clap_test_plugin.so",
            "opendeck_clap_test_plugin.dll",
            "libopendeck_clap_test_plugin.dylib",
        ]
        .iter()
        .map(|n| profile_dir.join(n))
        .find(|p| p.is_file())
        .expect("built fixture library");
        let dir = std::env::temp_dir().join(format!("odph-clap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        for name in ["gain", "crash", "hang"] {
            std::fs::copy(&lib, dir.join(format!("{name}.clap"))).expect("copy fixture");
        }
        dir
    })
}

fn gain_file() -> PathBuf {
    fixture_dir().join("gain.clap")
}

fn scanner(timeout: Duration) -> Scanner {
    Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_opendeck-plugin-scan")),
        timeout,
    }
}

fn key(id: &str) -> EffectKey {
    EffectKey {
        provider: "clap".into(),
        id: id.into(),
    }
}

fn ctx() -> ProcessContext {
    ProcessContext {
        sample_rate: SampleRate(48_000),
        bpm: None,
        beat_position: None,
        beat_division: Beats(1.0),
        playing: true,
    }
}

fn gain_info() -> PluginInfo {
    ClapBackend
        .scan_file(&gain_file())
        .expect("scan")
        .into_iter()
        .next()
        .expect("one plugin")
}

fn host() -> PluginHost {
    let main = Arc::new(ThreadMainThread::new("plugin-main").expect("thread"));
    let host = PluginHost::new(main, vec![Arc::new(ClapBackend)]);
    host.set_catalog(vec![gain_info()]);
    host
}

fn prepared(frames: usize) -> (Box<dyn opendeck_contracts::fx::Effect>, InstanceHandle) {
    let host = host();
    let (mut fx, handle) = host.create_effect(&key(GAIN_ID)).expect("create");
    fx.prepare(SampleRate(48_000), frames);
    assert_eq!(handle.status(), Ok(InstanceStatus::Active));
    (fx, handle)
}

fn run(fx: &mut dyn opendeck_contracts::fx::Effect, input: f32, frames: usize) -> Vec<f32> {
    let mut l = vec![input; frames];
    let mut r = vec![input; frames];
    fx.process(StereoBufferMut::new(&mut l, &mut r).expect("len"), &ctx());
    assert_eq!(l, r, "both channels get the same gain");
    l
}

// ---- scanning ----

#[test]
fn scan_lists_the_gain_plugin() {
    let info = gain_info();
    assert_eq!(info.key, key(GAIN_ID));
    assert_eq!(info.kind, PluginKind::Effect);
    assert_eq!(info.name, "Test Gain");
    assert_eq!(info.vendor, "OpenDeck tests");
    assert_eq!(info.version, "1.2.3");
    assert!(info.categories.iter().any(|c| c == "audio-effect"));
}

#[test]
fn subprocess_scan_validates_the_gain_plugin() {
    let record = scanner(Duration::from_secs(20)).scan("clap", &gain_file());
    assert_eq!(record.status, PluginScanStatus::Ok);
    assert_eq!(record.plugins.len(), 1);
    assert_eq!(record.plugins[0].key, key(GAIN_ID));
    assert!(record.is_current());
}

#[test]
fn crashing_plugin_is_blacklisted() {
    let record = scanner(Duration::from_secs(20)).scan("clap", &fixture_dir().join("crash.clap"));
    assert_eq!(record.status, PluginScanStatus::Crashed);
    assert!(record.status.blacklists());
    assert!(record.plugins.is_empty());
}

#[test]
fn hanging_plugin_times_out_and_is_blacklisted() {
    let started = std::time::Instant::now();
    let record = scanner(Duration::from_secs(2)).scan("clap", &fixture_dir().join("hang.clap"));
    assert_eq!(record.status, PluginScanStatus::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn scan_cache_reuses_blacklists_and_rescans_changes() {
    let dir = std::env::temp_dir().join(format!("odph-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for name in ["gain", "crash", "hang"] {
        std::fs::copy(
            fixture_dir().join(format!("{name}.clap")),
            dir.join(format!("{name}.clap")),
        )
        .expect("copy");
    }
    let formats = [FormatFolders {
        provider: "clap".into(),
        extension: "clap".into(),
        folders: vec![dir.clone()],
    }];
    let scanner = scanner(Duration::from_secs(3));
    let mut cache = ScanCache::new();

    let mut seen = Vec::new();
    let first = cache.rescan(&scanner, &formats, &RescanRequest::default(), |p| {
        seen.push(p.current)
    });
    assert_eq!((first.found, first.scanned, first.reused), (3, 3, 0));
    assert_eq!(seen.len(), 3);
    assert_eq!(cache.catalog().len(), 1);
    assert_eq!(cache.catalog()[0].key, key(GAIN_ID));
    let blacklisted: Vec<_> = cache
        .blacklist()
        .map(|r| (r.path.file_name().unwrap().to_owned(), r.status.clone()))
        .collect();
    assert_eq!(blacklisted.len(), 2);
    assert!(
        blacklisted
            .iter()
            .any(|(n, s)| n == "crash.clap" && *s == PluginScanStatus::Crashed)
    );
    assert!(
        blacklisted
            .iter()
            .any(|(n, s)| n == "hang.clap" && *s == PluginScanStatus::TimedOut)
    );

    // Persisted and reloaded: nothing is scanned again, blacklist included.
    let json = serde_json::to_string(&cache).expect("serialize");
    let mut cache: ScanCache = serde_json::from_str(&json).expect("deserialize");
    let second = cache.rescan(&scanner, &formats, &RescanRequest::default(), |_| {});
    assert_eq!((second.scanned, second.reused), (0, 3));
    assert_eq!(cache.blacklist().count(), 2);

    // A changed file is scanned again; the others are not.
    let mut bytes = std::fs::read(dir.join("gain.clap")).expect("read");
    bytes.extend_from_slice(b"padding");
    std::fs::write(dir.join("gain.clap"), bytes).expect("write");
    let third = cache.rescan(&scanner, &formats, &RescanRequest::default(), |_| {});
    assert_eq!((third.scanned, third.reused), (1, 2));

    // The user's retry scans exactly that file again.
    let retry = RescanRequest {
        all: false,
        retry: vec![dir.join("crash.clap")],
    };
    let fourth = cache.rescan(&scanner, &formats, &retry, |_| {});
    assert_eq!((fourth.scanned, fourth.reused), (1, 2));
    assert_eq!(cache.blacklist().count(), 2);

    // A deleted file drops its record.
    std::fs::remove_file(dir.join("crash.clap")).expect("remove");
    let fifth = cache.rescan(&scanner, &formats, &RescanRequest::default(), |_| {});
    assert_eq!((fifth.found, fifth.removed), (2, 1));
    assert_eq!(cache.blacklist().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- the effect adapter ----

#[test]
fn parameters_are_described_from_the_plugin() {
    let (fx, _handle) = prepared(64);
    let params = &fx.info().params;
    // The hidden read-only "Resets" parameter is not listed.
    assert_eq!(params.len(), 2);
    let gain = &params[0];
    assert_eq!((gain.id, gain.name.as_str()), (ParamId(0), "Gain"));
    assert_eq!((gain.min, gain.max, gain.default), (0.0, 2.0, 1.0));
    assert_eq!(gain.curve, ParamCurve::Linear);
    assert!(gain.is_valid());
    let mode = &params[1];
    assert_eq!((mode.id, mode.name.as_str()), (ParamId(1), "Mode"));
    assert_eq!(mode.curve, ParamCurve::Stepped { steps: 4 });
    assert!(mode.is_valid());
}

#[test]
fn audio_is_processed_in_place_through_the_plugin() {
    let (mut fx, _handle) = prepared(256);
    // Default gain 1.
    assert!(run(fx.as_mut(), 0.5, 256).iter().all(|s| *s == 0.5));
    // Normalized 0.25 of 0..2 is a gain of 0.5, applied from the next block.
    fx.set_parameter(ParamId(0), 0.25);
    assert!(
        run(fx.as_mut(), 0.8, 256)
            .iter()
            .all(|s| (*s - 0.4).abs() < 1e-6)
    );
    // Smaller blocks than prepared work too.
    assert_eq!(run(fx.as_mut(), 1.0, 17), vec![0.5; 17]);
}

#[test]
fn parameter_change_lands_on_its_frame_inside_the_block() {
    let (mut fx, handle) = prepared(256);
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
    // Two changes in one block, sent out of order.
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
}

#[test]
fn processing_does_not_allocate() {
    let (mut fx, handle) = prepared(512);
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
        // The mode echo exercises the plugin's output events too.
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
}

#[test]
fn unknown_and_oversized_input_is_safe() {
    let (mut fx, _handle) = prepared(64);
    fx.set_parameter(ParamId(999), 0.5);
    // A block larger than prepared passes the input through untouched.
    assert_eq!(run(fx.as_mut(), 0.7, 128), vec![0.7; 128]);
}

#[test]
fn plugin_output_events_reach_the_main_thread() {
    let (mut fx, handle) = prepared(64);
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
    let (mut fx, handle) = prepared(64);
    run(fx.as_mut(), 0.0, 64);
    assert!(handle.drain_param_output().expect("drain").is_empty());
    fx.reset();
    fx.reset();
    run(fx.as_mut(), 0.0, 64);
    // The plugin counts resets and reports the count (read-only parameter
    // id 2, range 0..1000) in the next block.
    let out = handle.drain_param_output().expect("drain");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, ParamId(2));
    assert!((out[0].normalized - 0.002).abs() < 1e-6, "{out:?}");
}

#[test]
fn latency_and_tail_come_from_the_plugin() {
    let (fx, handle) = prepared(64);
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
    let host = host();
    let (mut a, ha) = host.create_effect(&key(GAIN_ID)).expect("create a");
    a.prepare(SampleRate(48_000), 64);
    let default_state = ha.save_state().expect("save");
    assert_eq!(default_state.len(), 8);
    ha.send_param(ParamChange {
        id: ParamId(0),
        normalized: 0.75,
        offset: 0,
    })
    .expect("send");
    run(a.as_mut(), 0.0, 64);
    let saved = ha.save_state().expect("save");
    assert_ne!(saved, default_state);

    let (mut b, hb) = host.create_effect(&key(GAIN_ID)).expect("create b");
    hb.load_state(saved.clone()).expect("load");
    assert_eq!(hb.save_state().expect("save"), saved);
    b.prepare(SampleRate(44_100), 64);
    // Gain 0.75 of 0..2 is 1.5, restored from the state, not the default.
    assert!(
        run(b.as_mut(), 1.0, 64)
            .iter()
            .all(|s| (*s - 1.5).abs() < 1e-6)
    );
    assert!(hb.load_state(vec![1, 2, 3]).is_err());
}

#[test]
fn reprepare_at_another_rate_and_block_size_works() {
    let (mut fx, handle) = prepared(64);
    fx.prepare(SampleRate(96_000), 1024);
    assert_eq!(handle.status(), Ok(InstanceStatus::Active));
    assert_eq!(fx.latency_frames(), LATENCY);
    assert_eq!(run(fx.as_mut(), 0.5, 1024), vec![0.5; 1024]);
}

#[test]
fn dropping_the_effect_destroys_the_instance() {
    let (fx, handle) = prepared(64);
    drop(fx);
    // The destroy job runs on the main thread; a later call is queued behind it.
    assert!(matches!(
        handle.status(),
        Err(opendeck_plugin_host::PluginError::UnknownInstance(_))
    ));
}

#[test]
fn clap_effects_register_under_clap_keys() {
    let host = host();
    let mut registry = EffectRegistry::new();
    for factory in host.effect_factories() {
        registry.register(factory).expect("register");
    }
    let effects = registry.effects();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].key, key(GAIN_ID));
    assert_eq!(effects[0].name, "Test Gain");
    let mut fx = registry.create(&key(GAIN_ID)).expect("create");
    fx.prepare(SampleRate(48_000), 32);
    assert_eq!(fx.info().params.len(), 2);
    assert!(registry.create(&key("dev.nope")).is_err());
}

/// With a real plugin: `OPENDECK_TEST_CLAP=/path/to/Effect.clap cargo test
/// -p opendeck-plugin-host -- --ignored real_clap`. Not run in CI.
#[test]
#[ignore = "needs a real CLAP effect; set OPENDECK_TEST_CLAP"]
fn real_clap_effect_scans_and_processes() {
    let path = PathBuf::from(std::env::var("OPENDECK_TEST_CLAP").expect("OPENDECK_TEST_CLAP"));
    let record: ScanRecord = scanner(Duration::from_secs(30)).scan("clap", &path);
    assert_eq!(record.status, PluginScanStatus::Ok, "{record:?}");
    let info = record
        .plugins
        .iter()
        .find(|p| p.kind == PluginKind::Effect)
        .expect("an effect in the file")
        .clone();
    let main = Arc::new(ThreadMainThread::new("plugin-main").expect("thread"));
    let host = PluginHost::new(main, vec![Arc::new(ClapBackend)]);
    host.set_catalog(vec![info.clone()]);
    let (mut fx, _handle) = host.create_effect(&info.key).expect("create");
    fx.prepare(SampleRate(48_000), 512);
    let out = run(fx.as_mut(), 0.1, 512);
    assert!(out.iter().all(|s| s.is_finite()));
}
