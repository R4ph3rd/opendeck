//! The scan cache: stored [`ScanRecord`]s, the rules for refreshing them and
//! the catalog derived from them (ADR 0008).
//!
//! The cache is plain data (`Serialize`/`Deserialize`); app-core stores it as
//! JSON in the library settings (`plugins.scan`). A rescan walks the folders
//! of each format, then for every candidate file:
//!
//! - a record whose path, size and modification time still match is reused,
//!   including blacklisted ones (crash or timeout), so a bad plugin is not
//!   loaded again on every start;
//! - a changed or new file is scanned in the scanner subprocess;
//! - files the user asked to retry ([`RescanRequest::retry`]) or everything
//!   ([`RescanRequest::all`]) are scanned again regardless;
//! - records of files that no longer exist are dropped.
//!
//! Scanning is sequential, one subprocess per file, on the calling thread
//! (app-core runs it on a job thread and reports progress).

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::PathBuf;

use opendeck_contracts::plugin::{PluginInfo, PluginScanStatus};
use serde::{Deserialize, Serialize};

use crate::scan::{ScanRecord, Scanner, find_candidates};

/// What to scan for one plugin format.
#[derive(Debug, Clone)]
pub struct FormatFolders {
    /// Provider id (`"clap"`).
    pub provider: String,
    /// File or bundle extension without the dot.
    pub extension: String,
    /// Folders in priority order: default folders first, then user folders.
    pub folders: Vec<PathBuf>,
}

/// What a rescan must scan again even if the file did not change.
#[derive(Debug, Clone, Default)]
pub struct RescanRequest {
    /// Scan every file again.
    pub all: bool,
    /// Scan these files again (the user's "retry" on a blacklisted file).
    pub retry: Vec<PathBuf>,
}

/// Progress of a rescan, reported before each file is scanned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanProgress {
    /// Files handled so far (reused or scanned).
    pub done: usize,
    /// Files found in the folders.
    pub total: usize,
    /// The file about to be scanned.
    pub current: PathBuf,
}

/// Counts after a rescan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Files found in the folders.
    pub found: usize,
    /// Files scanned in the subprocess.
    pub scanned: usize,
    /// Files whose stored record was still valid.
    pub reused: usize,
    /// Stored records dropped because the file is gone.
    pub removed: usize,
}

/// Stored scan results.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanCache {
    records: Vec<ScanRecord>,
}

impl ScanCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// A cache from stored records.
    pub fn from_records(records: Vec<ScanRecord>) -> Self {
        Self { records }
    }

    /// All records, in scan order.
    pub fn records(&self) -> &[ScanRecord] {
        &self.records
    }

    /// Records of files that crashed or hung the scanner.
    pub fn blacklist(&self) -> impl Iterator<Item = &ScanRecord> {
        self.records.iter().filter(|r| r.status.blacklists())
    }

    /// Records of files that failed to scan without blacklisting
    /// (wrong architecture, not a plugin, failed validation).
    pub fn failed(&self) -> impl Iterator<Item = &ScanRecord> {
        self.records
            .iter()
            .filter(|r| matches!(r.status, PluginScanStatus::Failed { .. }))
    }

    /// Plugins from records with status `Ok`. When two files hold the same
    /// plugin key, the higher version wins, then the file found first (the
    /// earlier folder). Sorted by name, then key.
    pub fn catalog(&self) -> Vec<PluginInfo> {
        let mut out: Vec<PluginInfo> = Vec::new();
        for info in self
            .records
            .iter()
            .filter(|r| r.status == PluginScanStatus::Ok)
            .flat_map(|r| r.plugins.iter())
        {
            match out.iter_mut().find(|p| p.key == info.key) {
                Some(existing) => {
                    if compare_versions(&info.version, &existing.version) == Ordering::Greater {
                        *existing = info.clone();
                    }
                }
                None => out.push(info.clone()),
            }
        }
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.key.cmp(&b.key))
        });
        out
    }

    /// Refreshes the cache from the folders. See the module docs for the
    /// rules. `progress` is called before each file that is scanned.
    pub fn rescan(
        &mut self,
        scanner: &Scanner,
        formats: &[FormatFolders],
        request: &RescanRequest,
        mut progress: impl FnMut(ScanProgress),
    ) -> ScanSummary {
        let found: Vec<(String, PathBuf)> = formats
            .iter()
            .flat_map(|f| {
                find_candidates(&f.folders, &f.extension)
                    .into_iter()
                    .map(|p| (f.provider.clone(), p))
                    .collect::<Vec<_>>()
            })
            .collect();
        // Folder priority: earlier folders first within a format.
        let mut ordered = Vec::with_capacity(found.len());
        for f in formats {
            let mut of_format: Vec<&(String, PathBuf)> =
                found.iter().filter(|(p, _)| *p == f.provider).collect();
            of_format.sort_by_key(|(_, path)| folder_rank(&f.folders, path));
            ordered.extend(of_format.into_iter().cloned());
        }
        let wanted: HashSet<&PathBuf> = ordered.iter().map(|(_, p)| p).collect();
        let before = self.records.len();
        self.records.retain(|r| wanted.contains(&r.path));
        let removed = before - self.records.len();

        let mut summary = ScanSummary {
            found: ordered.len(),
            removed,
            ..ScanSummary::default()
        };
        let mut next = Vec::with_capacity(ordered.len());
        for (done, (provider, path)) in ordered.iter().enumerate() {
            let stored = self
                .records
                .iter()
                .position(|r| r.path == *path && r.provider == *provider)
                .map(|i| self.records.swap_remove(i));
            let forced = request.all || request.retry.iter().any(|p| p == path);
            match stored {
                Some(record) if !forced && record.is_current() => {
                    summary.reused += 1;
                    next.push(record);
                }
                _ => {
                    progress(ScanProgress {
                        done,
                        total: ordered.len(),
                        current: path.clone(),
                    });
                    summary.scanned += 1;
                    next.push(scanner.scan(provider, path));
                }
            }
        }
        self.records = next;
        summary
    }
}

/// Index of the first folder that contains `path` (later folders rank lower).
fn folder_rank(folders: &[PathBuf], path: &std::path::Path) -> usize {
    folders
        .iter()
        .position(|f| path.starts_with(f))
        .unwrap_or(folders.len())
}

/// Orders version strings by their dot-separated numeric parts (`1.10` is
/// newer than `1.9`); a non-numeric part compares as text.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let mut pa = a.split(['.', '-', '+']);
    let mut pb = b.split(['.', '-', '+']);
    loop {
        match (pa.next(), pb.next()) {
            (None, None) => return Ordering::Equal,
            (Some(x), None) => {
                if !is_zero(x) {
                    return Ordering::Greater;
                }
            }
            (None, Some(y)) => {
                if !is_zero(y) {
                    return Ordering::Less;
                }
            }
            (Some(x), Some(y)) => {
                let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(nx), Ok(ny)) => nx.cmp(&ny),
                    _ => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
        }
    }
}

fn is_zero(part: &str) -> bool {
    part.parse::<u64>() == Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::fx::EffectKey;
    use opendeck_contracts::plugin::PluginKind;

    fn info(id: &str, name: &str, version: &str, path: &str) -> PluginInfo {
        PluginInfo {
            key: EffectKey {
                provider: "clap".into(),
                id: id.into(),
            },
            kind: PluginKind::Effect,
            name: name.into(),
            vendor: String::new(),
            version: version.into(),
            path: path.into(),
            categories: Vec::new(),
            has_editor: false,
        }
    }

    fn record(path: &str, status: PluginScanStatus, plugins: Vec<PluginInfo>) -> ScanRecord {
        ScanRecord {
            provider: "clap".into(),
            path: path.into(),
            size: 1,
            modified: 1,
            status,
            plugins,
        }
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.1", "2.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("", "0.1"), Ordering::Less);
        assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
    }

    #[test]
    fn catalog_keeps_the_newer_version_then_the_first_file() {
        let cache = ScanCache::from_records(vec![
            record(
                "/a/x.clap",
                PluginScanStatus::Ok,
                vec![info("x", "X", "1.0", "/a/x.clap")],
            ),
            record(
                "/b/x.clap",
                PluginScanStatus::Ok,
                vec![info("x", "X", "1.1", "/b/x.clap")],
            ),
            record(
                "/a/y.clap",
                PluginScanStatus::Ok,
                vec![info("y", "Y", "1.0", "/a/y.clap")],
            ),
            record(
                "/b/y.clap",
                PluginScanStatus::Ok,
                vec![info("y", "Y", "1.0", "/b/y.clap")],
            ),
            record("/c/bad.clap", PluginScanStatus::Crashed, Vec::new()),
        ]);
        let catalog = cache.catalog();
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].path, "/b/x.clap");
        assert_eq!(catalog[1].path, "/a/y.clap");
        assert_eq!(cache.blacklist().count(), 1);
    }

    #[test]
    fn cache_round_trips_as_json() {
        let cache = ScanCache::from_records(vec![record(
            "/a/x.clap",
            PluginScanStatus::TimedOut,
            Vec::new(),
        )]);
        let json = serde_json::to_string(&cache).expect("serialize");
        let back: ScanCache = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, cache);
    }
}
