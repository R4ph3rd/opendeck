//! OpenDeck library: SQLite schema and migrations, import job queue,
//! playlists, saved sets and rekordbox XML import.
//!
//! The database runs in WAL mode with foreign keys on. Migrations are
//! append-only (ADR 0005). Nothing here runs on the audio thread or blocks the
//! UI thread: every [`Library`] call is synchronous and meant for a command or
//! worker thread, and the import queue runs on its own worker threads. See
//! `CONTEXT/library.md`.

mod analysis;
mod cues;
pub mod db;
mod error;
mod library;
mod paths;
mod playlists;
pub mod rekordbox;
mod sets;
mod tracks;
mod types;

pub use analysis::SaveOutcome;
pub use error::{LibraryError, Result};
pub use library::{Library, now_ms};
pub use paths::{normalize_path, normalize_path_string};
pub use types::*;

/// File name of the library database inside the app data directory.
pub const DATABASE_FILE_NAME: &str = "library.sqlite3";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_file_name_is_stable() {
        assert!(DATABASE_FILE_NAME.ends_with(".sqlite3"));
    }
}
