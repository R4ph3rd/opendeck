//! The [`Library`] handle: one writer connection, a small pool of readers.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::db;
use crate::error::{LibraryError, Result};

/// Readers kept open for reuse.
const READER_POOL: usize = 4;

struct Inner {
    /// The single writer (ADR 0005). Also serves reads of in-memory databases.
    writer: Mutex<Connection>,
    /// Read-only connections; empty and unused for in-memory databases.
    readers: Mutex<Vec<Connection>>,
    /// File path, `None` for in-memory.
    path: Option<PathBuf>,
}

/// Handle to the library database. Cheap to clone; share it across threads.
///
/// Every call is synchronous and may block on SQLite, so call it from a
/// worker or command thread, never from the audio thread. Writes are
/// serialised on one connection; reads use separate connections and run
/// concurrently with writes (WAL).
#[derive(Clone)]
pub struct Library {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Library {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Library")
            .field("path", &self.inner.path)
            .finish()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Current time as Unix milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

impl Library {
    /// Opens (creating and migrating if needed) the library file at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = db::open(path)?;
        Ok(Self {
            inner: Arc::new(Inner {
                writer: Mutex::new(conn),
                readers: Mutex::new(Vec::new()),
                path: Some(path.to_path_buf()),
            }),
        })
    }

    /// Opens a migrated in-memory library (tests, previews).
    pub fn open_in_memory() -> Result<Self> {
        let conn = db::open_in_memory()?;
        Ok(Self {
            inner: Arc::new(Inner {
                writer: Mutex::new(conn),
                readers: Mutex::new(Vec::new()),
                path: None,
            }),
        })
    }

    /// Runs `f` on the writer connection.
    pub(crate) fn write<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = lock(&self.inner.writer);
        f(&mut conn)
    }

    /// Runs `f` inside one write transaction (committed when `f` returns `Ok`).
    pub(crate) fn write_tx<T>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>,
    ) -> Result<T> {
        self.write(|conn| {
            let tx = conn.transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
    }

    /// Runs `f` on a read connection (the writer for in-memory databases).
    pub(crate) fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let Some(path) = &self.inner.path else {
            return f(&lock(&self.inner.writer));
        };
        let pooled = lock(&self.inner.readers).pop();
        let conn = match pooled {
            Some(c) => c,
            None => db::open_reader(path)?,
        };
        let out = f(&conn);
        let mut pool = lock(&self.inner.readers);
        if pool.len() < READER_POOL {
            pool.push(conn);
        }
        out
    }

    // ---- settings ----

    /// Reads a setting as raw JSON text.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.read(|c| {
            Ok(
                c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                    r.get(0)
                })
                .optional()?,
            )
        })
    }

    /// Writes a setting as raw JSON text.
    pub fn set_setting(&self, key: &str, json: &str) -> Result<()> {
        self.write(|c| {
            c.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                params![key, json, now_ms()],
            )?;
            Ok(())
        })
    }

    /// Removes a setting; returns whether it existed.
    pub fn delete_setting(&self, key: &str) -> Result<bool> {
        self.write(|c| Ok(c.execute("DELETE FROM settings WHERE key = ?1", [key])? > 0))
    }

    /// Reads and deserialises a setting.
    pub fn get_setting_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.get_setting(key)? {
            Some(text) => Ok(Some(serde_json::from_str(&text)?)),
            None => Ok(None),
        }
    }

    /// Serialises and writes a setting.
    pub fn set_setting_json<T: serde::Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set_setting(key, &serde_json::to_string(value)?)
    }

    /// Path of the database file, `None` for in-memory.
    pub fn path(&self) -> Option<&Path> {
        self.inner.path.as_deref()
    }

    /// Runs `PRAGMA optimize` and a passive WAL checkpoint; call on idle or quit.
    pub fn maintenance(&self) -> Result<()> {
        self.write(|c| {
            c.execute_batch("PRAGMA optimize; PRAGMA wal_checkpoint(PASSIVE);")?;
            Ok(())
        })
    }
}

pub(crate) fn not_found(what: &'static str) -> LibraryError {
    LibraryError::NotFound(what)
}
