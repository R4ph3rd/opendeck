//! Shared by the VST3 integration tests: builds the fixture library and
//! lays it out as `gain.vst3`, `crash.vst3` and `hang.vst3` bundles.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use opendeck_contracts::fx::{EffectKey, ProcessContext, StereoBufferMut};
use opendeck_contracts::plugin::PluginInfo;
use opendeck_contracts::{Beats, SampleRate};
use opendeck_plugin_host::backend::PluginBackend;
use opendeck_plugin_host::main_thread::ThreadMainThread;
use opendeck_plugin_host::scan::{Scanner, TargetOs, vst3_binary_path};
use opendeck_plugin_host::vst3::Vst3Backend;
use opendeck_plugin_host::{InstanceHandle, InstanceStatus, PluginHost};

/// Separate component and controller.
pub const GAIN_ID: &str = "6A317000000040008000000000000001";
/// One object for component and controller.
pub const SINGLE_ID: &str = "6A317000000040008000000000000003";
/// Mono only: refused by the host.
pub const MONO_ID: &str = "6A317000000040008000000000000004";
pub const LATENCY: u32 = 64;
pub const TAIL: u32 = 4800;

/// Builds the fixture cdylib (a no-op when cargo already built it) and
/// copies it into bundles in a temp folder.
pub fn fixture_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let exe = std::env::current_exe().expect("test exe");
        // target/<profile>/deps/<test exe>
        let profile_dir = exe.parent().and_then(Path::parent).expect("profile dir");
        let target_dir = profile_dir.parent().expect("target dir");
        let mut cmd = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cmd.args(["build", "-p", "opendeck-vst3-test-plugin", "--target-dir"])
            .arg(target_dir);
        if profile_dir.file_name().is_some_and(|n| n == "release") {
            cmd.arg("--release");
        }
        let status = cmd.status().expect("run cargo");
        assert!(status.success(), "fixture plugin build failed");
        let lib = [
            "libopendeck_vst3_test_plugin.so",
            "opendeck_vst3_test_plugin.dll",
            "libopendeck_vst3_test_plugin.dylib",
        ]
        .iter()
        .map(|n| profile_dir.join(n))
        .find(|p| p.is_file())
        .expect("built fixture library");
        let dir = std::env::temp_dir().join(format!("odph-vst3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        for name in ["gain", "crash", "hang"] {
            copy_bundle(&lib, &dir.join(format!("{name}.vst3")));
        }
        dir
    })
}

/// Lays `lib` out as the bundle `bundle`.
pub fn copy_bundle(lib: &Path, bundle: &Path) {
    let binary = vst3_binary_path(bundle, TargetOs::current(), std::env::consts::ARCH);
    std::fs::create_dir_all(binary.parent().expect("parent")).expect("mkdir bundle");
    std::fs::copy(lib, &binary).expect("copy fixture");
}

pub fn gain_bundle() -> PathBuf {
    fixture_dir().join("gain.vst3")
}

/// The library file inside a fixture bundle.
pub fn binary_of(bundle: &Path) -> PathBuf {
    vst3_binary_path(bundle, TargetOs::current(), std::env::consts::ARCH)
}

pub fn scanner(timeout: Duration) -> Scanner {
    Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_opendeck-plugin-scan")),
        timeout,
    }
}

pub fn key(id: &str) -> EffectKey {
    EffectKey {
        provider: "vst3".into(),
        id: id.into(),
    }
}

pub fn ctx() -> ProcessContext {
    ProcessContext {
        sample_rate: SampleRate(48_000),
        bpm: None,
        beat_position: None,
        beat_division: Beats(1.0),
        playing: true,
    }
}

pub fn scanned() -> Vec<PluginInfo> {
    Vst3Backend.scan_file(&gain_bundle()).expect("scan")
}

pub fn info_of(id: &str) -> PluginInfo {
    scanned()
        .into_iter()
        .find(|p| p.key.id == id)
        .expect("class in the fixture")
}

pub fn host() -> PluginHost {
    let main = Arc::new(ThreadMainThread::new("plugin-main").expect("thread"));
    let host = PluginHost::new(main, vec![Arc::new(Vst3Backend)]);
    host.set_catalog(scanned());
    host
}

pub fn prepared_on(
    host: &PluginHost,
    id: &str,
    frames: usize,
) -> (Box<dyn opendeck_contracts::fx::Effect>, InstanceHandle) {
    let (mut fx, handle) = host.create_effect(&key(id)).expect("create");
    fx.prepare(SampleRate(48_000), frames);
    assert_eq!(handle.status(), Ok(InstanceStatus::Active));
    (fx, handle)
}

pub fn run(fx: &mut dyn opendeck_contracts::fx::Effect, input: f32, frames: usize) -> Vec<f32> {
    let mut l = vec![input; frames];
    let mut r = vec![input; frames];
    fx.process(StereoBufferMut::new(&mut l, &mut r).expect("len"), &ctx());
    assert_eq!(l, r, "both channels get the same gain");
    l
}
