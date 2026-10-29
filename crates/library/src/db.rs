//! Database connection and schema migrations (ADR 0005).
//!
//! Migrations live in `crates/library/migrations/NNNN_name.sql`, are embedded
//! at compile time and applied in order by `rusqlite_migration`, which tracks
//! the schema version in SQLite's `user_version`. They are append-only: a
//! released migration is never edited; fixes go in a new file.

use std::path::Path;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

/// Every migration, in order. Index `n` brings the schema to version `n + 1`.
const MIGRATION_LIST: &[M<'static>] = &[M::up(include_str!("../migrations/0001_init.sql"))];

/// Schema version after all migrations (`PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 1;

/// Error opening or migrating the database.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Migration error.
    #[error("migration: {0}")]
    Migration(#[from] rusqlite_migration::Error),
}

/// The migration set, for validation in tests and tooling.
pub fn migrations() -> Migrations<'static> {
    Migrations::from_slice(MIGRATION_LIST)
}

/// Per-connection settings. WAL is persistent in the file but is set on
/// every open so a fresh file gets it; it is a no-op for in-memory databases.
fn configure(conn: &Connection) -> Result<(), DbError> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

/// Opens (creating if needed) the library at `path` and migrates it to
/// [`SCHEMA_VERSION`].
pub fn open(path: &Path) -> Result<Connection, DbError> {
    let mut conn = Connection::open(path)?;
    configure(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

/// Opens a migrated in-memory database (tests, previews).
pub fn open_in_memory() -> Result<Connection, DbError> {
    let mut conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("user_version")
    }

    #[test]
    fn migrations_are_valid() {
        migrations().validate().expect("migrations validate");
    }

    #[test]
    fn in_memory_reaches_latest_with_foreign_keys() {
        let conn = open_in_memory().expect("open");
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .expect("pragma");
        assert_eq!(fk, 1);
        for table in [
            "tracks",
            "artwork",
            "analysis",
            "analysis_peaks",
            "cues",
            "playlists",
            "playlist_tracks",
            "saved_sets",
            "saved_set_tracks",
            "import_jobs",
            "import_job_items",
            "settings",
            "fx_presets",
            "midi_mapping_overrides",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |r| r.get(0),
                )
                .expect("query");
            assert_eq!(n, 1, "missing table {table}");
        }
    }

    fn insert_track(conn: &Connection, path: &str) -> i64 {
        conn.execute(
            "INSERT INTO tracks (path, file_size, file_mtime, date_added) VALUES (?1, 1, 0, 0)",
            [path],
        )
        .expect("insert track");
        conn.last_insert_rowid()
    }

    #[test]
    fn constraints_and_cascades() {
        let conn = open_in_memory().expect("open");
        let track = insert_track(&conn, "/music/a.flac");

        // Path is unique.
        assert!(
            conn.execute(
                "INSERT INTO tracks (path, file_size, file_mtime, date_added) VALUES ('/music/a.flac', 1, 0, 0)",
                [],
            )
            .is_err()
        );

        // One hot cue per slot; memory cues have no slot; loops need an end.
        let hot = "INSERT INTO cues (track_id, kind, hot_index, start_secs, created_at) VALUES (?1, 'hot', 0, 1.0, 0)";
        conn.execute(hot, [track]).expect("first hot cue");
        assert!(conn.execute(hot, [track]).is_err(), "duplicate hot slot");
        assert!(
            conn.execute(
                "INSERT INTO cues (track_id, kind, hot_index, start_secs, created_at) VALUES (?1, 'memory', 1, 1.0, 0)",
                [track],
            )
            .is_err(),
            "memory cue with slot"
        );
        assert!(
            conn.execute(
                "INSERT INTO cues (track_id, kind, start_secs, created_at) VALUES (?1, 'loop', 1.0, 0)",
                [track],
            )
            .is_err(),
            "loop without end"
        );

        // Folders cannot hold tracks.
        conn.execute(
            "INSERT INTO playlists (kind, name, position, created_at, updated_at) VALUES ('folder', 'Sets', 0, 0, 0)",
            [],
        )
        .expect("folder");
        let folder = conn.last_insert_rowid();
        assert!(
            conn.execute(
                "INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at) VALUES (?1, 0, ?2, 0)",
                [folder, track],
            )
            .is_err()
        );

        // Deleting a track removes its cues.
        conn.execute("DELETE FROM tracks WHERE id = ?1", [track])
            .expect("delete");
        let cues: i64 = conn
            .query_row("SELECT count(*) FROM cues", [], |r| r.get(0))
            .expect("count");
        assert_eq!(cues, 0);
    }

    #[test]
    fn file_database_uses_wal() {
        let dir = std::env::temp_dir().join(format!("opendeck-lib-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(crate::DATABASE_FILE_NAME);
        {
            let conn = open(&path).expect("open file db");
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .expect("pragma");
            assert_eq!(mode.to_lowercase(), "wal");
        }
        // Re-opening an up-to-date database is a no-op.
        let conn = open(&path).expect("reopen");
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
