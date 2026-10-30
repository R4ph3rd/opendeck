//! Directory listing for the in-app file explorer pane: folders, supported
//! audio files and the playlist files the importers read.

use std::path::{Path, PathBuf};

use crate::dto::{DirListing, FsEntry, FsEntryKind};
use crate::types::CoreError;

/// Playlist and collection file extensions the importers read.
const PLAYLIST_EXTENSIONS: &[&str] = &["m3u", "m3u8", "xml"];

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Filesystem roots: drive letters on Windows, `/` elsewhere.
fn roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        ('A'..='Z')
            .map(|c| PathBuf::from(format!("{c}:\\")))
            .filter(|p| p.exists())
            .collect()
    } else {
        vec![PathBuf::from("/")]
    }
}

fn entry(name: String, path: &Path, kind: FsEntryKind) -> FsEntry {
    FsEntry {
        name,
        path: path.to_string_lossy().into_owned(),
        kind,
    }
}

/// Lists `path`: folders first, then audio and playlist files, each sorted by
/// name; hidden entries are left out. `None` lists the home folder (the roots
/// when there is none); an empty path lists the roots.
pub fn list_directory(path: Option<&Path>) -> Result<DirListing, CoreError> {
    let target = match path {
        Some(p) if p.as_os_str().is_empty() => None,
        Some(p) => Some(p.to_path_buf()),
        None => home_dir(),
    };
    let Some(dir) = target else {
        let entries = roots()
            .iter()
            .map(|r| entry(r.to_string_lossy().into_owned(), r, FsEntryKind::Dir))
            .collect();
        return Ok(DirListing {
            path: String::new(),
            parent: None,
            entries,
        });
    };
    let read = std::fs::read_dir(&dir)
        .map_err(|e| CoreError::Media(format!("cannot open {}: {e}", dir.display())))?;
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = item.path();
        // `metadata` follows symlinks, so a linked folder lists as a folder.
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            dirs.push(entry(name, &path, FsEntryKind::Dir));
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if opendeck_media::is_supported_extension(&ext) {
            files.push(entry(name, &path, FsEntryKind::Audio));
        } else if PLAYLIST_EXTENSIONS.contains(&ext.as_str()) {
            files.push(entry(name, &path, FsEntryKind::Playlist));
        }
    }
    let by_name = |a: &FsEntry, b: &FsEntry| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    dirs.sort_by(by_name);
    files.sort_by(by_name);
    dirs.extend(files);
    // A root's parent is the list of roots (an empty path).
    let parent = dir
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .or_else(|| Some(String::new()));
    Ok(DirListing {
        path: dir.to_string_lossy().into_owned(),
        parent,
        entries: dirs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_folders_then_supported_files() {
        let dir = std::env::temp_dir().join(format!("opendeck-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("b-folder")).unwrap();
        std::fs::create_dir_all(dir.join(".hidden")).unwrap();
        for name in ["z.wav", "a.MP3", "notes.txt", "set.m3u8", ".secret.wav"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let listing = list_directory(Some(&dir)).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["b-folder", "a.MP3", "set.m3u8", "z.wav"]);
        assert_eq!(listing.entries[0].kind, FsEntryKind::Dir);
        assert_eq!(listing.entries[2].kind, FsEntryKind::Playlist);
        assert_eq!(listing.entries[3].kind, FsEntryKind::Audio);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_folder_is_a_media_error() {
        let err = list_directory(Some(Path::new("/nonexistent/opendeck"))).unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }

    #[test]
    fn empty_path_lists_roots() {
        let listing = list_directory(Some(Path::new(""))).unwrap();
        assert!(listing.path.is_empty());
        assert!(!listing.entries.is_empty());
    }
}
