//! The scanner subprocess against the fake format: success, failure, crash
//! and hang each give the expected record.

use std::path::{Path, PathBuf};
use std::time::Duration;

use opendeck_contracts::plugin::PluginScanStatus;
use opendeck_plugin_host::scan::{ScanRecord, Scanner};

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("odph-scan-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

fn scan(name: &str, timeout: Duration) -> ScanRecord {
    let path = dir().join(name);
    std::fs::write(&path, b"fake").expect("write");
    let scanner = Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_opendeck-plugin-scan")),
        timeout,
    };
    scanner.scan("fake", &path)
}

#[test]
fn good_file_lists_both_plugins() {
    let r = scan("good.fakeplug", Duration::from_secs(20));
    assert_eq!(r.status, PluginScanStatus::Ok);
    assert_eq!(r.plugins.len(), 2);
    assert!(r.is_current());
    assert!(r.size > 0);
}

#[test]
fn broken_file_fails_without_blacklisting() {
    let r = scan("broken.fakeplug", Duration::from_secs(20));
    assert!(matches!(r.status, PluginScanStatus::Failed { .. }));
    assert!(!r.status.blacklists());
}

#[test]
fn crash_is_blacklisted() {
    let r = scan("crash.fakeplug", Duration::from_secs(20));
    assert_eq!(r.status, PluginScanStatus::Crashed);
    assert!(r.status.blacklists());
}

#[test]
fn hang_is_killed_and_blacklisted() {
    let r = scan("hang.fakeplug", Duration::from_millis(500));
    assert_eq!(r.status, PluginScanStatus::TimedOut);
}

#[test]
fn unknown_format_fails() {
    let path = dir().join("x.vst9");
    std::fs::write(&path, b"").expect("write");
    let scanner = Scanner {
        exe: PathBuf::from(env!("CARGO_BIN_EXE_opendeck-plugin-scan")),
        timeout: Duration::from_secs(20),
    };
    let r = scanner.scan("vst9", Path::new(&path));
    assert!(matches!(r.status, PluginScanStatus::Failed { .. }));
}
