//! Discovery: folder walk and M3U/M3U8 parsing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::rekordbox::location_to_path;

/// True when `path` has a supported audio extension.
pub(crate) fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(opendeck_media::is_supported_extension)
}

/// Recursively lists supported audio files under `root`, in sorted order per
/// directory. Hidden entries (leading dot, which includes macOS `._` files)
/// and symlinked directories are skipped, so loops cannot happen. Calls
/// `on_file` per file and stops early when `stop` returns true.
pub(crate) fn walk(
    root: &Path,
    stop: &dyn Fn() -> bool,
    on_file: &mut dyn FnMut(PathBuf),
) -> io::Result<()> {
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        if stop() {
            return Ok(());
        }
        let mut entries: Vec<_> = match fs::read_dir(&dir) {
            Ok(rd) => rd.filter_map(Result::ok).collect(),
            // An unreadable subfolder is skipped; an unreadable root is an error.
            Err(e) if dir == root => return Err(e),
            Err(_) => continue,
        };
        entries.sort_by_key(fs::DirEntry::file_name);
        let mut subdirs = Vec::new();
        for entry in entries {
            if stop() {
                return Ok(());
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                subdirs.push(path);
            } else if (ft.is_file() || (ft.is_symlink() && path.is_file())) && is_audio(&path) {
                on_file(path);
            }
        }
        // Depth-first in name order: push reversed so the first subdir pops first.
        dirs.extend(subdirs.into_iter().rev());
    }
    Ok(())
}

/// Parses an M3U/M3U8 file into track paths in file order.
///
/// Lines starting with `#` (including `#EXTINF`) and blank lines are skipped.
/// Relative entries resolve against the playlist's folder; `file://` URLs are
/// decoded. The file is read as UTF-8, lossily (plain `.m3u` files in a legacy
/// code page may show replacement characters).
pub(crate) fn parse_m3u(path: &Path) -> io::Result<Vec<PathBuf>> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let p = if l.starts_with("file://") {
                location_to_path(l)
            } else {
                PathBuf::from(l.replace('\\', if cfg!(windows) { "\\" } else { "/" }))
            };
            if p.is_absolute() { p } else { base.join(p) }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m3u_skips_comments_and_resolves_relative_paths() {
        let dir = std::env::temp_dir().join(format!("opendeck-lib-m3u-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("set.m3u8");
        fs::write(
            &file,
            "\u{feff}#EXTM3U\n#EXTINF:1,a\nsub/a.wav\n\n/abs/b.flac\nfile:///c%20d/e.mp3\n",
        )
        .expect("write");
        let got = parse_m3u(&file).expect("parse");
        assert_eq!(
            got,
            vec![
                dir.join("sub/a.wav"),
                PathBuf::from("/abs/b.flac"),
                PathBuf::from("/c d/e.mp3")
            ]
        );
    }

    #[test]
    fn walk_filters_extensions_and_hidden_entries() {
        let root = std::env::temp_dir().join(format!("opendeck-lib-walk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("b")).expect("dir");
        fs::create_dir_all(root.join(".hidden")).expect("dir");
        for f in [
            "a.wav",
            "notes.txt",
            "b/z.FLAC",
            "b/._z.flac",
            ".hidden/x.wav",
        ] {
            fs::write(root.join(f), b"x").expect("write");
        }
        let mut found = Vec::new();
        walk(&root, &|| false, &mut |p| found.push(p)).expect("walk");
        assert_eq!(found, vec![root.join("a.wav"), root.join("b/z.FLAC")]);
    }
}
