//! Path normalization for the `tracks.path` key.

use std::path::{Component, Path, PathBuf};

/// Absolute, lexically cleaned form of `path` (`.` and `..` resolved, no
/// symlink resolution, no case folding). Used as the unique track key.
///
/// TODO(verify): case-insensitive filesystems (Windows, default macOS) can
/// still produce two keys for one file; relinking by content hash covers the
/// moved-file case only. See `CONTEXT/library.md` open questions.
pub fn normalize_path(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::ParentDir => {
                if !out.pop() {
                    out.push(comp.as_os_str());
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// [`normalize_path`] as the string stored in the database.
pub fn normalize_path_string(path: &Path) -> String {
    normalize_path(path).to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_dots() {
        let p = normalize_path(Path::new("/music/./a/../b/song.mp3"));
        // `absolute` adds the current drive on Windows, so build the expectation the same way.
        let expected = std::path::absolute(Path::new("/music/b/song.mp3")).expect("absolute");
        assert_eq!(p, expected);
    }

    #[test]
    fn relative_becomes_absolute() {
        assert!(normalize_path(Path::new("x.wav")).is_absolute());
    }
}
