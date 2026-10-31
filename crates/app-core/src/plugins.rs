//! Plugin scan service (ADR 0008, wave 9a-1).
//!
//! Owns the [`PluginHost`], the [`ScanCache`] (persisted as JSON in the
//! library settings, key [`SCAN_KEY`]) and the user's plugin folders
//! ([`FOLDERS_KEY`]). A rescan runs on its own thread, one scanner
//! subprocess per file, and reports progress to a sink the shell sets (the
//! shell turns it into an event). A finished scan replaces the host catalog
//! and bumps [`PluginService::catalog_version`]; the control thread sees the
//! bump on its next tick and refreshes the FX effect list.
//!
//! The service is cheap to clone. [`PluginService::default`] is a disabled
//! service (no plugins, no scanning) used by tests and tools.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use opendeck_contracts::fx::EffectFactory;
use opendeck_contracts::plugin::{PluginInfo, PluginScanStatus};
use opendeck_library::Library;
use opendeck_plugin_host::main_thread::MainThread;
use opendeck_plugin_host::scan::{ScanRecord, Scanner};
use opendeck_plugin_host::scan_cache::{FormatFolders, RescanRequest, ScanCache};
use opendeck_plugin_host::{PluginBackend, PluginHost, compiled_backends};
use serde::{Deserialize, Serialize};

use crate::types::CoreError;

/// Library settings key of the stored scan records.
pub const SCAN_KEY: &str = "plugins.scan";
/// Library settings key of the user's extra plugin folders.
pub const FOLDERS_KEY: &str = "plugins.folders";

/// One scanned file, for the blacklist and failure lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginFileDto {
    /// Provider id of the format (`"clap"`).
    pub provider: String,
    /// File or bundle path.
    pub path: String,
    /// Why it is listed.
    pub status: PluginScanStatus,
    /// Plugins it holds (0 unless the scan succeeded).
    pub plugins: u32,
}

impl From<&ScanRecord> for PluginFileDto {
    fn from(r: &ScanRecord) -> Self {
        Self {
            provider: r.provider.clone(),
            path: r.path.display().to_string(),
            status: r.status.clone(),
            plugins: u32::try_from(r.plugins.len()).unwrap_or(u32::MAX),
        }
    }
}

/// Progress of a running scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginScanProgressDto {
    /// Files handled before the current one.
    pub done: u32,
    /// Files found in the folders.
    pub total: u32,
    /// File being scanned.
    pub current: String,
}

/// Everything the plugin settings view and the plugin browser show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PluginsStateDto {
    /// A scan is running.
    pub scanning: bool,
    /// Progress of the running scan.
    pub progress: Option<PluginScanProgressDto>,
    /// Plugins found by the last scan, usable as FX.
    pub plugins: Vec<PluginInfo>,
    /// Files that crashed or hung the scanner; not loaded until retried or changed.
    pub blacklist: Vec<PluginFileDto>,
    /// Files that failed to scan without blacklisting (wrong architecture, not a plugin).
    pub failed: Vec<PluginFileDto>,
    /// Folders the user added.
    pub folders: Vec<String>,
    /// Default folders of the compiled formats on this OS.
    pub default_folders: Vec<String>,
    /// The `opendeck-plugin-scan` binary was found next to the app.
    pub scanner_available: bool,
}

type Sink = Arc<dyn Fn(PluginsStateDto) + Send + Sync>;

#[derive(Default)]
struct Progress {
    scanning: bool,
    current: Option<PluginScanProgressDto>,
}

struct Inner {
    host: PluginHost,
    backends: Vec<Arc<dyn PluginBackend>>,
    library: Option<Library>,
    scanner: Option<Scanner>,
    cache: Mutex<ScanCache>,
    folders: Mutex<Vec<PathBuf>>,
    progress: Mutex<Progress>,
    version: AtomicU64,
    sink: Mutex<Option<Sink>>,
}

/// See the module docs.
#[derive(Clone, Default)]
pub struct PluginService {
    inner: Option<Arc<Inner>>,
}

impl std::fmt::Debug for PluginService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginService")
            .field("enabled", &self.inner.is_some())
            .finish()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PluginService {
    /// A service for the formats compiled into this build. `main` runs the
    /// plugin main thread (the platform UI thread in the app); `scanner` is
    /// the scanner binary (`None` disables scanning, stored results still
    /// load). Stored scan results and folders are read from `library`.
    pub fn new(
        main: Arc<dyn MainThread>,
        library: Option<Library>,
        scanner: Option<Scanner>,
    ) -> Self {
        let backends = compiled_backends();
        let host = PluginHost::new(main, backends.clone());
        let cache = library
            .as_ref()
            .and_then(|l| l.get_setting_json::<ScanCache>(SCAN_KEY).ok().flatten())
            .unwrap_or_default();
        let folders = library
            .as_ref()
            .and_then(|l| {
                l.get_setting_json::<Vec<PathBuf>>(FOLDERS_KEY)
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
        host.set_catalog(cache.catalog());
        host.set_param_table(cache.param_table());
        Self {
            inner: Some(Arc::new(Inner {
                host,
                backends,
                library,
                scanner,
                cache: Mutex::new(cache),
                folders: Mutex::new(folders),
                progress: Mutex::new(Progress::default()),
                version: AtomicU64::new(1),
                sink: Mutex::new(None),
            })),
        }
    }

    /// The plugin host, for instance handles (state, editors). `None` when disabled.
    pub fn host(&self) -> Option<PluginHost> {
        self.inner.as_ref().map(|i| i.host.clone())
    }

    /// Effect factories to register in the `EffectRegistry`: one per compiled
    /// format, provider `clap` (keys `clap:<plugin id>`). Empty when disabled.
    pub fn effect_factories(&self) -> Vec<Box<dyn EffectFactory>> {
        self.inner
            .as_ref()
            .map(|i| i.host.effect_factories())
            .unwrap_or_default()
    }

    /// Counts catalog changes; the control thread compares it each tick.
    pub fn catalog_version(&self) -> u64 {
        self.inner
            .as_ref()
            .map_or(0, |i| i.version.load(Ordering::Acquire))
    }

    /// Sets the function called with the new state whenever a scan starts,
    /// advances or ends.
    pub fn set_sink(&self, sink: impl Fn(PluginsStateDto) + Send + Sync + 'static) {
        if let Some(i) = &self.inner {
            *lock(&i.sink) = Some(Arc::new(sink));
        }
    }

    /// Current state.
    pub fn state(&self) -> PluginsStateDto {
        match &self.inner {
            Some(i) => i.state(),
            None => PluginsStateDto {
                scanning: false,
                progress: None,
                plugins: Vec::new(),
                blacklist: Vec::new(),
                failed: Vec::new(),
                folders: Vec::new(),
                default_folders: Vec::new(),
                scanner_available: false,
            },
        }
    }

    /// Starts a scan on its own thread. With the default request only new
    /// and changed files are scanned; blacklisted files stay blacklisted.
    /// Fails if a scan is running or the scanner binary is missing.
    pub fn rescan(&self, request: RescanRequest) -> Result<(), CoreError> {
        let inner = self
            .inner
            .clone()
            .ok_or_else(|| CoreError::Invalid("plugins are not available".into()))?;
        let scanner = inner
            .scanner
            .clone()
            .ok_or_else(|| CoreError::Invalid("the plugin scanner was not found".into()))?;
        {
            let mut p = lock(&inner.progress);
            if p.scanning {
                return Err(CoreError::Invalid(
                    "a plugin scan is already running".into(),
                ));
            }
            p.scanning = true;
            p.current = None;
        }
        inner.emit();
        std::thread::Builder::new()
            .name("opendeck-plugin-scan".into())
            .spawn(move || inner.run_scan(&scanner, &request))
            .map(|_| ())
            .map_err(|e| CoreError::Invalid(format!("could not start the plugin scan: {e}")))
    }

    /// Scans `path` again even if it did not change (the user's retry on a
    /// blacklisted file).
    pub fn retry(&self, path: PathBuf) -> Result<(), CoreError> {
        self.rescan(RescanRequest {
            all: false,
            retry: vec![path],
        })
    }

    /// Replaces the user's plugin folders, stores them and rescans.
    pub fn set_folders(&self, folders: Vec<PathBuf>) -> Result<(), CoreError> {
        let inner = self
            .inner
            .as_ref()
            .ok_or_else(|| CoreError::Invalid("plugins are not available".into()))?;
        if let Some(lib) = &inner.library {
            lib.set_setting_json(FOLDERS_KEY, &folders)
                .map_err(|e| CoreError::Invalid(e.to_string()))?;
        }
        *lock(&inner.folders) = folders;
        self.rescan(RescanRequest::default())
    }
}

impl Inner {
    fn formats(&self) -> Vec<FormatFolders> {
        let user = lock(&self.folders).clone();
        self.backends
            .iter()
            .map(|b| {
                let mut folders = b.default_folders();
                folders.extend(user.iter().cloned());
                FormatFolders {
                    provider: b.provider().to_owned(),
                    extension: b.extension().to_owned(),
                    folders,
                }
            })
            .collect()
    }

    fn state(&self) -> PluginsStateDto {
        let cache = lock(&self.cache);
        let progress = lock(&self.progress);
        let paths = |v: Vec<PathBuf>| {
            v.into_iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        };
        PluginsStateDto {
            scanning: progress.scanning,
            progress: progress.current.clone(),
            plugins: cache.catalog(),
            blacklist: cache.blacklist().map(PluginFileDto::from).collect(),
            failed: cache.failed().map(PluginFileDto::from).collect(),
            folders: paths(lock(&self.folders).clone()),
            default_folders: paths(
                self.backends
                    .iter()
                    .flat_map(|b| b.default_folders())
                    .collect(),
            ),
            scanner_available: self.scanner.is_some(),
        }
    }

    fn emit(&self) {
        let sink = lock(&self.sink).clone();
        if let Some(sink) = sink {
            sink(self.state());
        }
    }

    fn run_scan(&self, scanner: &Scanner, request: &RescanRequest) {
        let mut cache = lock(&self.cache).clone();
        let formats = self.formats();
        let summary = cache.rescan(scanner, &formats, request, |p| {
            lock(&self.progress).current = Some(PluginScanProgressDto {
                done: u32::try_from(p.done).unwrap_or(u32::MAX),
                total: u32::try_from(p.total).unwrap_or(u32::MAX),
                current: p.current.display().to_string(),
            });
            self.emit();
        });
        tracing::info!(
            found = summary.found,
            scanned = summary.scanned,
            reused = summary.reused,
            removed = summary.removed,
            "plugin scan finished"
        );
        if let Some(lib) = &self.library
            && let Err(err) = lib.set_setting_json(SCAN_KEY, &cache)
        {
            tracing::warn!(%err, "could not store the plugin scan results");
        }
        self.host.set_catalog(cache.catalog());
        self.host.set_param_table(cache.param_table());
        *lock(&self.cache) = cache;
        self.version.fetch_add(1, Ordering::AcqRel);
        {
            let mut p = lock(&self.progress);
            p.scanning = false;
            p.current = None;
        }
        self.emit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_plugin_host::main_thread::ThreadMainThread;

    #[test]
    fn disabled_service_is_empty() {
        let service = PluginService::default();
        assert!(service.effect_factories().is_empty());
        assert_eq!(service.catalog_version(), 0);
        assert!(service.state().plugins.is_empty());
        assert!(service.rescan(RescanRequest::default()).is_err());
    }

    #[test]
    fn missing_scanner_refuses_to_scan_but_loads_stored_results() {
        let main = Arc::new(ThreadMainThread::new("test-main").expect("thread"));
        let service = PluginService::new(main, None, None);
        let state = service.state();
        assert!(!state.scanner_available);
        assert!(!state.scanning);
        assert!(service.rescan(RescanRequest::default()).is_err());
        assert_eq!(service.effect_factories().len(), 1);
    }
}
