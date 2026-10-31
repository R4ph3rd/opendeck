//! Plugin scanner subprocess (ADR 0008).
//!
//! `opendeck-plugin-scan <provider> <path>` loads one plugin file, validates
//! its plugins and prints one `ScanReport` as JSON on stdout. It exits 0
//! whenever it could print a report, also for a failed scan; any other exit
//! is a crash for the host. The provider `fake` is the test format.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use opendeck_plugin_host::compiled_backends;
use opendeck_plugin_host::fake::FakeBackend;
use opendeck_plugin_host::scan::{SCAN_PROTOCOL, ScanReport, scan_in_process};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let report = match args.as_slice() {
        [provider, path] => {
            let mut backends = compiled_backends();
            backends.push(Arc::new(FakeBackend));
            match backends.iter().find(|b| b.provider() == provider) {
                Some(b) => scan_in_process(b.as_ref(), Path::new(path)),
                None => failed(format!("plugin format {provider} is not available")),
            }
        }
        _ => failed("usage: opendeck-plugin-scan <provider> <path>".into()),
    };
    match serde_json::to_string(&report) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(_) => ExitCode::from(3),
    }
}

fn failed(error: String) -> ScanReport {
    ScanReport {
        protocol: SCAN_PROTOCOL,
        plugins: Vec::new(),
        error: Some(error),
    }
}
