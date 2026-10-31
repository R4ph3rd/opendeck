//! Crash-safe database startup: `PRAGMA quick_check` before the library opens.
//!
//! A file that fails the check (or is not a database at all) is moved aside
//! with its WAL and SHM files, so the library creates a fresh one and the
//! user can still recover the old file by hand.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// What the startup check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbHealth {
    /// No file yet; the library creates one.
    Missing,
    /// `quick_check` passed.
    Healthy,
    /// The file was damaged and moved to `backup`.
    Recovered {
        /// Where the damaged file went.
        backup: PathBuf,
        /// What `quick_check` or SQLite reported.
        reason: String,
    },
}

impl DbHealth {
    /// Message for the user when the database was replaced.
    pub fn notice(&self) -> Option<String> {
        match self {
            Self::Recovered { backup, .. } => Some(format!(
                "The library database was damaged and has been replaced by an empty one. \
                 The old file is kept at {}. Import your music folders again.",
                backup.display()
            )),
            _ => None,
        }
    }
}

/// Runs `PRAGMA quick_check`; `Ok(())` when the file is sound, `Err(reason)` otherwise.
pub fn quick_check(path: &Path) -> Result<(), String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("PRAGMA quick_check")
        .map_err(|e| e.to_string())?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    if rows.len() == 1 && rows[0] == "ok" {
        Ok(())
    } else {
        Err(rows.join("; "))
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Checks the database at `path`; a damaged one is backed up and removed.
pub fn check_and_recover(path: &Path, stamp: u64) -> std::io::Result<DbHealth> {
    if !path.is_file() {
        return Ok(DbHealth::Missing);
    }
    let Err(reason) = quick_check(path) else {
        return Ok(DbHealth::Healthy);
    };
    tracing::error!(%reason, path = %path.display(), "database failed quick_check");
    let backup = with_suffix(path, &format!(".corrupt-{stamp}"));
    std::fs::rename(path, &backup)?;
    for ext in ["-wal", "-shm"] {
        let side = with_suffix(path, ext);
        if side.exists() {
            let _ = std::fs::rename(&side, with_suffix(&backup, ext));
        }
    }
    Ok(DbHealth::Recovered { backup, reason })
}

/// Seconds since the Unix epoch, for backup names.
pub fn unix_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("opendeck-dbguard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = temp_dir("missing");
        assert_eq!(
            check_and_recover(&dir.join("library.db"), 1).expect("check"),
            DbHealth::Missing
        );
    }

    #[test]
    fn healthy_database_is_left_alone() {
        let dir = temp_dir("healthy");
        let path = dir.join("library.db");
        let conn = Connection::open(&path).expect("open");
        conn.execute_batch("CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1);")
            .expect("create");
        drop(conn);
        assert_eq!(
            check_and_recover(&path, 1).expect("check"),
            DbHealth::Healthy
        );
        assert!(path.is_file());
    }

    #[test]
    fn garbage_file_is_backed_up_and_removed() {
        let dir = temp_dir("garbage");
        let path = dir.join("library.db");
        std::fs::write(&path, vec![0xAB; 4096]).expect("write");
        let health = check_and_recover(&path, 42).expect("check");
        let DbHealth::Recovered { backup, .. } = &health else {
            panic!("expected recovery, got {health:?}");
        };
        assert!(backup.ends_with("library.db.corrupt-42"));
        assert!(backup.is_file());
        assert!(!path.exists());
        assert!(health.notice().expect("notice").contains("corrupt-42"));
    }

    #[test]
    fn damaged_pages_are_recovered() {
        let dir = temp_dir("damaged");
        let path = dir.join("library.db");
        let conn = Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE t (a TEXT);
             WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 5000)
             INSERT INTO t SELECT hex(randomblob(64)) FROM c;",
        )
        .expect("fill");
        drop(conn);
        let mut bytes = std::fs::read(&path).expect("read");
        // Damage the middle pages but keep the header, as a partial write would.
        for b in bytes.iter_mut().skip(4096).take(65536) {
            *b = 0xFF;
        }
        std::fs::write(&path, bytes).expect("write");
        let health = check_and_recover(&path, 7).expect("check");
        assert!(matches!(health, DbHealth::Recovered { .. }), "{health:?}");
    }
}
