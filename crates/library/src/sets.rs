//! Saved sets: the ordered history of a session plus a JSON snapshot.

use opendeck_contracts::ids::TrackId;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{LibraryError, Result};
use crate::library::{Library, not_found, now_ms};
use crate::types::{SavedSet, SavedSetEntry};

fn append_on(conn: &Connection, set: i64, e: &SavedSetEntry) -> Result<()> {
    if e.deck.is_some_and(|d| d > 3) {
        return Err(LibraryError::Invalid("deck must be 0 to 3".into()));
    }
    let position: i64 = conn.query_row(
        "SELECT count(*) FROM saved_set_tracks WHERE set_id = ?1",
        [set],
        |r| r.get(0),
    )?;
    // Snapshot title and artist from the track unless the caller gave them.
    let (mut title, mut artist) = (e.title.clone(), e.artist.clone());
    if let Some(t) = e.track {
        let snap: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT title, artist FROM tracks WHERE id = ?1",
                [t.0],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((t_title, t_artist)) = snap {
            title = title.or(t_title);
            artist = artist.or(t_artist);
        }
    }
    conn.execute(
        "INSERT INTO saved_set_tracks (set_id, position, track_id, title, artist, deck, played_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            set,
            position,
            e.track.map(|t| t.0),
            title,
            artist,
            e.deck,
            e.played_at
        ],
    )?;
    Ok(())
}

impl Library {
    /// Creates a saved set with its entries (in play order).
    pub fn create_saved_set(
        &self,
        name: &str,
        notes: &str,
        session_state: Option<&str>,
        started_at: Option<i64>,
        entries: &[SavedSetEntry],
    ) -> Result<i64> {
        self.write_tx(|tx| {
            let now = now_ms();
            tx.execute(
                "INSERT INTO saved_sets (name, notes, session_state, started_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![name, notes, session_state, started_at, now],
            )?;
            let id = tx.last_insert_rowid();
            for e in entries {
                append_on(tx, id, e)?;
            }
            Ok(id)
        })
    }

    /// Appends a played track to a set (live logging).
    pub fn append_to_saved_set(&self, set: i64, entry: &SavedSetEntry) -> Result<()> {
        self.write_tx(|tx| {
            let exists: bool = tx
                .query_row("SELECT 1 FROM saved_sets WHERE id = ?1", [set], |_| {
                    Ok(true)
                })
                .optional()?
                .unwrap_or(false);
            if !exists {
                return Err(not_found("saved set"));
            }
            append_on(tx, set, entry)?;
            tx.execute(
                "UPDATE saved_sets SET updated_at = ?2 WHERE id = ?1",
                params![set, now_ms()],
            )?;
            Ok(())
        })
    }

    /// Replaces name, notes and session snapshot.
    pub fn update_saved_set(
        &self,
        set: i64,
        name: &str,
        notes: &str,
        session_state: Option<&str>,
    ) -> Result<()> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE saved_sets SET name = ?2, notes = ?3, session_state = ?4, updated_at = ?5 \
                 WHERE id = ?1",
                params![set, name, notes, session_state, now_ms()],
            )?;
            if n == 0 {
                Err(not_found("saved set"))
            } else {
                Ok(())
            }
        })
    }

    /// Deletes a set and its entries.
    pub fn delete_saved_set(&self, set: i64) -> Result<bool> {
        self.write(|c| Ok(c.execute("DELETE FROM saved_sets WHERE id = ?1", [set])? > 0))
    }

    /// All sets, newest first.
    pub fn saved_sets(&self) -> Result<Vec<SavedSet>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT s.id, s.name, s.notes, s.session_state, s.started_at, s.created_at, s.updated_at, \
                 (SELECT count(*) FROM saved_set_tracks e WHERE e.set_id = s.id) \
                 FROM saved_sets s ORDER BY s.created_at DESC, s.id DESC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(SavedSet {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        notes: r.get(2)?,
                        session_state: r.get(3)?,
                        started_at: r.get(4)?,
                        created_at: r.get(5)?,
                        updated_at: r.get(6)?,
                        track_count: r.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Entries of a set in play order.
    pub fn saved_set_entries(&self, set: i64) -> Result<Vec<SavedSetEntry>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT track_id, title, artist, deck, played_at FROM saved_set_tracks \
                 WHERE set_id = ?1 ORDER BY position",
            )?;
            let rows = stmt
                .query_map([set], |r| {
                    Ok(SavedSetEntry {
                        track: r.get::<_, Option<i64>>(0)?.map(TrackId),
                        title: r.get(1)?,
                        artist: r.get(2)?,
                        deck: r.get(3)?,
                        played_at: r.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }
}
