//! Plugin discovery: default folders, candidate files, and the scanner
//! subprocess that loads them.
//!
//! The app never loads an unscanned plugin. [`Scanner::scan`] runs the
//! `opendeck-plugin-scan` binary once per file:
//!
//! ```text
//! opendeck-plugin-scan <provider> <path>
//! ```
//!
//! The scanner loads the file, lists its plugins, then validates each one
//! ([`scan_in_process`]): instantiate, activate at 48 kHz / 512 frames,
//! process [`VALIDATION_BLOCKS`] blocks (an impulse for effects, a note for
//! instruments), check the output is finite, deactivate. It prints one
//! [`ScanReport`] as JSON on stdout and exits 0, also when the report holds
//! an error. A non-zero exit or a signal is a crash; no exit within the
//! timeout is a hang. Both blacklist the file ([`PluginScanStatus::blacklists`]).
//!
//! app-core stores [`ScanRecord`]s (library settings) and rescans a file only
//! when its size or modification time changed, or when the user asks.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, UNIX_EPOCH};

use opendeck_contracts::Beats;
use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::{ProcessContext, StereoBufferMut};
use opendeck_contracts::instrument::NoteEvent;
use opendeck_contracts::plugin::{PluginInfo, PluginKind, PluginScanStatus};
use serde::{Deserialize, Serialize};

use crate::PluginError;
use crate::backend::{PluginBackend, ProcessBlock};

/// Version of the stdout protocol; a report with another version is a
/// failed scan (app and scanner from different builds).
pub const SCAN_PROTOCOL: u32 = 1;

/// Default time one file may take in the scanner. Product choice
/// (`TODO(verify)` against slow-loading plugin suites in wave 9a-1).
pub const DEFAULT_SCAN_TIMEOUT: Duration = Duration::from_secs(30);

/// Blocks processed per plugin during validation.
pub const VALIDATION_BLOCKS: usize = 8;

const VALIDATION_RATE: SampleRate = SampleRate(48_000);
const VALIDATION_FRAMES: usize = 512;

/// What the scanner prints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    /// [`SCAN_PROTOCOL`].
    pub protocol: u32,
    /// Plugins that loaded and passed validation.
    pub plugins: Vec<PluginInfo>,
    /// Why the file (or every plugin in it) failed, if it did.
    pub error: Option<String>,
}

/// The stored outcome of scanning one file. Persisted by app-core as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRecord {
    /// Provider id of the format.
    pub provider: String,
    /// File or bundle.
    pub path: PathBuf,
    /// Size in bytes when scanned (0 for bundles, which are directories).
    pub size: u64,
    /// Modification time when scanned, seconds since the Unix epoch.
    pub modified: u64,
    /// Outcome.
    pub status: PluginScanStatus,
    /// Plugins found; empty unless `status` is `Ok`.
    pub plugins: Vec<PluginInfo>,
}

impl ScanRecord {
    /// True if the file still has the size and time it had when scanned.
    pub fn is_current(&self) -> bool {
        file_stamp(&self.path).is_some_and(|(s, m)| s == self.size && m == self.modified)
    }
}

/// `(size, mtime seconds)` of a file or bundle directory.
fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    let size = if meta.is_dir() { 0 } else { meta.len() };
    Some((size, modified))
}

/// Runs the scanner binary with a timeout. Control thread (or a scan job
/// thread); each call blocks until the child exits or is killed.
#[derive(Debug, Clone)]
pub struct Scanner {
    /// Path of `opendeck-plugin-scan` (shipped next to the app binary).
    pub exe: PathBuf,
    /// Time one file may take.
    pub timeout: Duration,
}

impl Scanner {
    /// Scans one file in a subprocess.
    pub fn scan(&self, provider: &str, path: &Path) -> ScanRecord {
        let (size, modified) = file_stamp(path).unwrap_or((0, 0));
        let (status, plugins) = match self.run(provider, path) {
            Ok(Outcome::Report(r)) if r.protocol != SCAN_PROTOCOL => (
                PluginScanStatus::Failed {
                    reason: format!("scanner protocol {} (expected {SCAN_PROTOCOL})", r.protocol),
                },
                Vec::new(),
            ),
            Ok(Outcome::Report(r)) => match r.error {
                Some(reason) if r.plugins.is_empty() => {
                    (PluginScanStatus::Failed { reason }, Vec::new())
                }
                _ => (PluginScanStatus::Ok, r.plugins),
            },
            Ok(Outcome::Crashed) => (PluginScanStatus::Crashed, Vec::new()),
            Ok(Outcome::TimedOut) => (PluginScanStatus::TimedOut, Vec::new()),
            Err(e) => (
                PluginScanStatus::Failed {
                    reason: e.to_string(),
                },
                Vec::new(),
            ),
        };
        ScanRecord {
            provider: provider.to_owned(),
            path: path.to_owned(),
            size,
            modified,
            status,
            plugins,
        }
    }

    fn run(&self, provider: &str, path: &Path) -> Result<Outcome, PluginError> {
        let mut child = Command::new(&self.exe)
            .arg(provider)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                PluginError::Scanner(format!("cannot start {}: {e}", self.exe.display()))
            })?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| PluginError::Scanner("no stdout".into()))?;
        // Read on a thread so a chatty child cannot fill the pipe and stall.
        let reader = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout.read_to_end(&mut buf);
            buf
        });
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|e| PluginError::Scanner(e.to_string()))?
            {
                break Some(status);
            }
            if start.elapsed() >= self.timeout {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let out = reader.join().unwrap_or_default();
        match status {
            None => Ok(Outcome::TimedOut),
            Some(s) if !s.success() => Ok(Outcome::Crashed),
            Some(_) => serde_json::from_slice(&out)
                .map(Outcome::Report)
                .map_err(|e| PluginError::Scanner(format!("bad report: {e}"))),
        }
    }
}

enum Outcome {
    Report(ScanReport),
    Crashed,
    TimedOut,
}

/// Scans and validates one file in this process. Only the scanner binary
/// calls this: it runs plugin code. Runs on the calling thread, which acts
/// as the plugin main thread.
pub fn scan_in_process(backend: &dyn PluginBackend, path: &Path) -> ScanReport {
    let listed = match backend.scan_file(path) {
        Ok(p) => p,
        Err(e) => {
            return ScanReport {
                protocol: SCAN_PROTOCOL,
                plugins: Vec::new(),
                error: Some(e.to_string()),
            };
        }
    };
    let mut plugins = Vec::new();
    let mut errors = Vec::new();
    for info in listed {
        match validate(backend, &info) {
            Ok(()) => plugins.push(info),
            // Backends without instances yet are listed unvalidated.
            Err(PluginError::NotYetImplemented(_)) => plugins.push(info),
            Err(e) => errors.push(format!("{}: {e}", info.name)),
        }
    }
    ScanReport {
        protocol: SCAN_PROTOCOL,
        plugins,
        error: (!errors.is_empty()).then(|| errors.join("; ")),
    }
}

fn validate(backend: &dyn PluginBackend, info: &PluginInfo) -> Result<(), PluginError> {
    let mut instance = backend.instantiate(info)?;
    let mut processor = instance.activate(VALIDATION_RATE, VALIDATION_FRAMES)?;
    let ctx = ProcessContext {
        sample_rate: VALIDATION_RATE,
        bpm: None,
        beat_position: None,
        beat_division: Beats(1.0),
        playing: false,
    };
    let mut in_l = vec![0.0_f32; VALIDATION_FRAMES];
    let in_r = in_l.clone();
    if let Some(s) = in_l.first_mut() {
        *s = 1.0;
    }
    let mut out_l = vec![0.0_f32; VALIDATION_FRAMES];
    let mut out_r = vec![0.0_f32; VALIDATION_FRAMES];
    let note = NoteEvent::on(0, 0, 60, 0.8).into_iter().collect::<Vec<_>>();
    let mut sink = Vec::new();
    let mut finite = true;
    for block in 0..VALIDATION_BLOCKS {
        let output = StereoBufferMut::new(&mut out_l, &mut out_r)
            .ok_or_else(|| PluginError::Scanner("buffer".into()))?;
        let first = block == 0;
        processor.process(ProcessBlock {
            input: (info.kind == PluginKind::Effect).then_some((&in_l[..], &in_r[..])),
            output,
            params: &[],
            notes: if first && info.kind == PluginKind::Instrument {
                &note
            } else {
                &[]
            },
            params_out: &mut VecSink(&mut sink),
            ctx: &ctx,
        });
        finite &= out_l.iter().chain(&out_r).all(|s| s.is_finite());
        if first {
            in_l.fill(0.0);
        }
    }
    instance.deactivate(processor);
    if finite {
        Ok(())
    } else {
        Err(PluginError::Plugin("non-finite output".into()))
    }
}

struct VecSink<'a>(&'a mut Vec<crate::backend::ParamChange>);

impl crate::backend::ParamSink for VecSink<'_> {
    fn push(&mut self, change: crate::backend::ParamChange) {
        self.0.push(change);
    }
}

/// Plugin files and bundles under `folders`, recursively, for one
/// extension. A directory named `*.<ext>` is a bundle: it is a candidate
/// and is not entered. Symlinks are followed up to a depth of 16. Sorted,
/// without duplicates.
pub fn find_candidates(folders: &[PathBuf], extension: &str) -> Vec<PathBuf> {
    fn walk(dir: &Path, ext: &str, depth: u32, out: &mut Vec<PathBuf>) {
        if depth > 16 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let has_ext = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext));
            if has_ext {
                out.push(path);
            } else if path.is_dir() {
                walk(&path, ext, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    for f in folders {
        walk(f, extension, 0, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

/// Operating system for the folder rules (pure functions, testable on any OS).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux and other Unix.
    Linux,
}

impl TargetOs {
    /// The OS this binary runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// CLAP default folders, from the comment in `clap/entry.h` (CLAP 1.2):
/// Linux `~/.clap`, `/usr/lib/clap`; Windows `%COMMONPROGRAMFILES%\CLAP`,
/// `%LOCALAPPDATA%\Programs\Common\CLAP`; macOS
/// `/Library/Audio/Plug-Ins/CLAP`, `~/Library/Audio/Plug-Ins/CLAP`; then
/// every entry of `CLAP_PATH` (`;`-separated on Windows, `:` elsewhere).
pub fn clap_folders(os: TargetOs, env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let home = env("HOME").map(PathBuf::from);
    let mut out = Vec::new();
    match os {
        TargetOs::Linux => {
            out.extend(home.map(|h| h.join(".clap")));
            out.push(PathBuf::from("/usr/lib/clap"));
        }
        TargetOs::Windows => {
            out.extend(env("COMMONPROGRAMFILES").map(|p| PathBuf::from(p).join("CLAP")));
            out.extend(
                env("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs\\Common\\CLAP")),
            );
        }
        TargetOs::MacOs => {
            out.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
            out.extend(home.map(|h| h.join("Library/Audio/Plug-Ins/CLAP")));
        }
    }
    if let Some(list) = env("CLAP_PATH") {
        let sep = if os == TargetOs::Windows { ';' } else { ':' };
        out.extend(
            list.to_string_lossy()
                .split(sep)
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    out
}

/// VST3 default folders, from the VST 3 SDK hosting code
/// (`public.sdk/source/vst/hosting/module_{win32,mac,linux}`): Windows
/// `FOLDERID_UserProgramFilesCommon\VST3` (`%LOCALAPPDATA%\Programs\Common\VST3`)
/// and `FOLDERID_ProgramFilesCommon\VST3` (`%COMMONPROGRAMFILES%\VST3`);
/// macOS `~/Library/Audio/Plug-Ins/VST3` and `/Library/Audio/Plug-Ins/VST3`;
/// Linux `~/.vst3`, `/usr/lib64/vst3`, `/usr/lib/vst3`,
/// `/usr/local/lib64/vst3`, `/usr/local/lib/vst3`. The SDK's
/// application-folder entry is left out: OpenDeck ships no plugins.
pub fn vst3_folders(os: TargetOs, env: impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let home = env("HOME").map(PathBuf::from);
    let mut out = Vec::new();
    match os {
        TargetOs::Windows => {
            out.extend(
                env("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs\\Common\\VST3")),
            );
            out.extend(env("COMMONPROGRAMFILES").map(|p| PathBuf::from(p).join("VST3")));
        }
        TargetOs::MacOs => {
            out.extend(home.map(|h| h.join("Library/Audio/Plug-Ins/VST3")));
            out.push(PathBuf::from("/Library/Audio/Plug-Ins/VST3"));
        }
        TargetOs::Linux => {
            out.extend(home.map(|h| h.join(".vst3")));
            for p in [
                "/usr/lib64/vst3",
                "/usr/lib/vst3",
                "/usr/local/lib64/vst3",
                "/usr/local/lib/vst3",
            ] {
                out.push(PathBuf::from(p));
            }
        }
    }
    out
}

/// The library inside a VST3 bundle, following the SDK hosting code:
/// Windows `<bundle>/Contents/<arch>-win/<bundle file name>` (`x86_64-win`,
/// `arm64-win`; a `.vst3` that is a plain file is the DLL itself), Linux
/// `<bundle>/Contents/<uname -m>-linux/<stem>.so`, macOS
/// `<bundle>/Contents/MacOS/<stem>` (`TODO(verify)`: the SDK reads
/// `CFBundleExecutable` from `Info.plist`; the stem is the usual value).
/// `arch` is Rust's `std::env::consts::ARCH`.
pub fn vst3_binary_path(bundle: &Path, os: TargetOs, arch: &str) -> PathBuf {
    let stem = bundle.file_stem().map(OsString::from).unwrap_or_default();
    let contents = bundle.join("Contents");
    match os {
        TargetOs::Windows => {
            let dir = match arch {
                "aarch64" => "arm64-win",
                _ => "x86_64-win",
            };
            let name = bundle.file_name().map(OsString::from).unwrap_or_default();
            contents.join(dir).join(name)
        }
        TargetOs::Linux => {
            let mut name = stem;
            name.push(".so");
            contents.join(format!("{arch}-linux")).join(name)
        }
        TargetOs::MacOs => contents.join("MacOS").join(stem),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.into())
    }

    #[test]
    fn clap_folders_follow_entry_h() {
        let linux = clap_folders(
            TargetOs::Linux,
            env_of(&[("HOME", "/home/u"), ("CLAP_PATH", "/a::/b")]),
        );
        assert_eq!(
            linux,
            ["/home/u/.clap", "/usr/lib/clap", "/a", "/b"].map(PathBuf::from)
        );
        let mac = clap_folders(TargetOs::MacOs, env_of(&[("HOME", "/Users/u")]));
        assert_eq!(
            mac,
            [
                "/Library/Audio/Plug-Ins/CLAP",
                "/Users/u/Library/Audio/Plug-Ins/CLAP"
            ]
            .map(PathBuf::from)
        );
        let win = clap_folders(
            TargetOs::Windows,
            env_of(&[
                ("COMMONPROGRAMFILES", "C:\\CF"),
                ("LOCALAPPDATA", "C:\\LA"),
                ("CLAP_PATH", "D:\\x;E:\\y"),
            ]),
        );
        assert_eq!(win.len(), 4);
        assert_eq!(win[2], PathBuf::from("D:\\x"));
    }

    #[test]
    fn vst3_folders_follow_the_sdk() {
        let linux = vst3_folders(TargetOs::Linux, env_of(&[("HOME", "/home/u")]));
        assert_eq!(linux.first(), Some(&PathBuf::from("/home/u/.vst3")));
        assert_eq!(linux.len(), 5);
        let win = vst3_folders(
            TargetOs::Windows,
            env_of(&[("COMMONPROGRAMFILES", "C:\\CF"), ("LOCALAPPDATA", "C:\\LA")]),
        );
        assert_eq!(win.len(), 2);
        assert!(vst3_folders(TargetOs::MacOs, env_of(&[])).len() == 1);
    }

    #[test]
    fn vst3_bundle_layout() {
        let b = Path::new("/p/Gain.vst3");
        assert_eq!(
            vst3_binary_path(b, TargetOs::Linux, "x86_64"),
            PathBuf::from("/p/Gain.vst3/Contents/x86_64-linux/Gain.so")
        );
        assert_eq!(
            vst3_binary_path(b, TargetOs::Windows, "x86_64"),
            PathBuf::from("/p/Gain.vst3/Contents/x86_64-win/Gain.vst3")
        );
        assert_eq!(
            vst3_binary_path(b, TargetOs::MacOs, "aarch64"),
            PathBuf::from("/p/Gain.vst3/Contents/MacOS/Gain")
        );
    }

    #[test]
    fn candidates_stop_at_bundles() {
        let root = std::env::temp_dir().join(format!("odph-cand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let bundle = root.join("vendor/Synth.clap");
        std::fs::create_dir_all(bundle.join("Contents")).expect("mkdir");
        std::fs::write(bundle.join("Contents/inner.clap"), b"").expect("write");
        std::fs::write(root.join("Fx.CLAP"), b"").expect("write");
        std::fs::write(root.join("readme.txt"), b"").expect("write");
        let found = find_candidates(&[root.clone(), root.clone()], "clap");
        assert_eq!(found, vec![root.join("Fx.CLAP"), bundle]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
