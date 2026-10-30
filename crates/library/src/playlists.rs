//! Playlists and folders: tree, entries, ordering.
//!
//! Positions are dense integers (`0..n`) among siblings or entries. Schema v2
//! dropped the uniqueness of `(playlist_id, position)`, so a shift is one
//! `UPDATE` and a reorder rewrites only the rows whose index changed.

use opendeck_contracts::ids::{PlaylistId, TrackId};
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{LibraryError, Result};
use crate::library::{Library, not_found, now_ms};
use crate::tracks::{ROW_COLUMNS, row_to_track_row_at};
use crate::types::{PlaylistEntry, PlaylistKind, PlaylistNode};

fn kind_of(conn: &Connection, id: PlaylistId) -> Result<PlaylistKind> {
    let kind: Option<String> = conn
        .query_row("SELECT kind FROM playlists WHERE id = ?1", [id.0], |r| {
            r.get(0)
        })
        .optional()?;
    match kind.as_deref() {
        Some("folder") => Ok(PlaylistKind::Folder),
        Some("playlist") => Ok(PlaylistKind::Playlist),
        Some(other) => Err(LibraryError::Corrupt(format!("playlist kind {other}"))),
        None => Err(not_found("playlist")),
    }
}

fn sibling_ids(conn: &Connection, parent: Option<PlaylistId>) -> Result<Vec<i64>> {
    let mut stmt = conn
        .prepare_cached("SELECT id FROM playlists WHERE parent_id IS ?1 ORDER BY position, id")?;
    let rows = stmt
        .query_map([parent.map(|p| p.0)], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn renumber_siblings(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut stmt =
        conn.prepare_cached("UPDATE playlists SET position = ?2 WHERE id = ?1 AND position <> ?2")?;
    for (i, id) in ids.iter().enumerate() {
        stmt.execute(params![id, i as i64])?;
    }
    Ok(())
}

fn create_node(
    conn: &Connection,
    parent: Option<PlaylistId>,
    kind: PlaylistKind,
    name: &str,
) -> Result<PlaylistId> {
    if let Some(p) = parent
        && kind_of(conn, p)? != PlaylistKind::Folder
    {
        return Err(LibraryError::Invalid("parent must be a folder".into()));
    }
    let position: i64 = conn.query_row(
        "SELECT count(*) FROM playlists WHERE parent_id IS ?1",
        [parent.map(|p| p.0)],
        |r| r.get(0),
    )?;
    let now = now_ms();
    conn.execute(
        "INSERT INTO playlists (parent_id, kind, name, position, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![
            parent.map(|p| p.0),
            match kind {
                PlaylistKind::Folder => "folder",
                PlaylistKind::Playlist => "playlist",
            },
            name,
            position,
            now
        ],
    )?;
    Ok(PlaylistId(conn.last_insert_rowid()))
}

fn entry_ids(conn: &Connection, playlist: PlaylistId) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "SELECT entry_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position, entry_id",
    )?;
    let rows = stmt
        .query_map([playlist.0], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn require_playlist(conn: &Connection, id: PlaylistId) -> Result<()> {
    match kind_of(conn, id)? {
        PlaylistKind::Playlist => Ok(()),
        PlaylistKind::Folder => Err(LibraryError::Invalid("folders hold no tracks".into())),
    }
}

/// Appends `tracks` (or inserts at `index`) on `conn`; returns the new entry ids.
pub(crate) fn add_tracks_on(
    conn: &Connection,
    playlist: PlaylistId,
    tracks: &[TrackId],
    index: Option<usize>,
) -> Result<Vec<i64>> {
    require_playlist(conn, playlist)?;
    let len: i64 = conn.query_row(
        "SELECT count(*) FROM playlist_tracks WHERE playlist_id = ?1",
        [playlist.0],
        |r| r.get(0),
    )?;
    let at = index.map_or(len, |i| (i as i64).min(len));
    let n = tracks.len() as i64;
    if at < len {
        conn.execute(
            "UPDATE playlist_tracks SET position = position + ?2 \
             WHERE playlist_id = ?1 AND position >= ?3",
            params![playlist.0, n, at],
        )?;
    }
    let now = now_ms();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let mut ids = Vec::with_capacity(tracks.len());
    for (i, t) in tracks.iter().enumerate() {
        stmt.execute(params![playlist.0, at + i as i64, t.0, now])?;
        ids.push(conn.last_insert_rowid());
    }
    conn.execute(
        "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
        params![playlist.0, now],
    )?;
    Ok(ids)
}

pub(crate) fn create_playlist_on(
    conn: &Connection,
    parent: Option<PlaylistId>,
    name: &str,
) -> Result<PlaylistId> {
    create_node(conn, parent, PlaylistKind::Playlist, name)
}

pub(crate) fn create_folder_on(
    conn: &Connection,
    parent: Option<PlaylistId>,
    name: &str,
) -> Result<PlaylistId> {
    create_node(conn, parent, PlaylistKind::Folder, name)
}

impl Library {
    /// Creates an empty playlist at the end of `parent` (top level when `None`).
    pub fn create_playlist(&self, parent: Option<PlaylistId>, name: &str) -> Result<PlaylistId> {
        self.write_tx(|tx| create_playlist_on(tx, parent, name))
    }

    /// Creates an empty folder at the end of `parent` (top level when `None`).
    pub fn create_folder(&self, parent: Option<PlaylistId>, name: &str) -> Result<PlaylistId> {
        self.write_tx(|tx| create_folder_on(tx, parent, name))
    }

    /// Renames a playlist or folder.
    pub fn rename_playlist(&self, id: PlaylistId, name: &str) -> Result<()> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.0, name, now_ms()],
            )?;
            if n == 0 {
                Err(not_found("playlist"))
            } else {
                Ok(())
            }
        })
    }

    /// Moves a node under `new_parent` (a folder, or `None` for top level) at
    /// sibling index `index` (clamped to the end). Refuses cycles.
    pub fn move_playlist(
        &self,
        id: PlaylistId,
        new_parent: Option<PlaylistId>,
        index: usize,
    ) -> Result<()> {
        self.write_tx(|tx| {
            kind_of(tx, id)?;
            if let Some(p) = new_parent {
                if kind_of(tx, p)? != PlaylistKind::Folder {
                    return Err(LibraryError::Invalid("parent must be a folder".into()));
                }
                // Walk up from the new parent: meeting `id` would make a cycle.
                let mut cur = Some(p);
                while let Some(c) = cur {
                    if c == id {
                        return Err(LibraryError::Invalid(
                            "cannot move a folder into itself".into(),
                        ));
                    }
                    cur = tx
                        .query_row(
                            "SELECT parent_id FROM playlists WHERE id = ?1",
                            [c.0],
                            |r| r.get::<_, Option<i64>>(0),
                        )?
                        .map(PlaylistId);
                }
            }
            let old_parent: Option<i64> = tx.query_row(
                "SELECT parent_id FROM playlists WHERE id = ?1",
                [id.0],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE playlists SET parent_id = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.0, new_parent.map(|p| p.0), now_ms()],
            )?;
            // Close the gap in the old list, then place the node in the new one.
            let mut old = sibling_ids(tx, old_parent.map(PlaylistId))?;
            old.retain(|&x| x != id.0);
            renumber_siblings(tx, &old)?;
            let mut list = sibling_ids(tx, new_parent)?;
            list.retain(|&x| x != id.0);
            list.insert(index.min(list.len()), id.0);
            renumber_siblings(tx, &list)
        })
    }

    /// Deletes a node with its subtree and entries (tracks stay in the library).
    pub fn delete_playlist(&self, id: PlaylistId) -> Result<()> {
        self.write_tx(|tx| {
            let parent: Option<i64> = tx
                .query_row(
                    "SELECT parent_id FROM playlists WHERE id = ?1",
                    [id.0],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| not_found("playlist"))?;
            tx.execute("DELETE FROM playlists WHERE id = ?1", [id.0])?;
            let rest = sibling_ids(tx, parent.map(PlaylistId))?;
            renumber_siblings(tx, &rest)
        })
    }

    /// The whole tree as a flat list ordered by parent then position.
    pub fn playlist_tree(&self) -> Result<Vec<PlaylistNode>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT p.id, p.parent_id, p.kind, p.name, p.position, \
                 (SELECT count(*) FROM playlist_tracks e WHERE e.playlist_id = p.id) \
                 FROM playlists p ORDER BY p.parent_id IS NOT NULL, p.parent_id, p.position, p.id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    let kind: String = r.get(2)?;
                    Ok(PlaylistNode {
                        id: PlaylistId(r.get(0)?),
                        parent: r.get::<_, Option<i64>>(1)?.map(PlaylistId),
                        kind: if kind == "folder" {
                            PlaylistKind::Folder
                        } else {
                            PlaylistKind::Playlist
                        },
                        name: r.get(3)?,
                        position: r.get(4)?,
                        track_count: r.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Adds tracks at `index` (clamped; the end when `None`); returns entry ids.
    pub fn add_tracks_to_playlist(
        &self,
        playlist: PlaylistId,
        tracks: &[TrackId],
        index: Option<usize>,
    ) -> Result<Vec<i64>> {
        self.write_tx(|tx| add_tracks_on(tx, playlist, tracks, index))
    }

    /// Removes entries by id and closes the gaps.
    pub fn remove_playlist_entries(&self, playlist: PlaylistId, entries: &[i64]) -> Result<()> {
        self.write_tx(|tx| {
            let mut stmt = tx.prepare_cached(
                "DELETE FROM playlist_tracks WHERE entry_id = ?1 AND playlist_id = ?2",
            )?;
            for e in entries {
                stmt.execute(params![e, playlist.0])?;
            }
            drop(stmt);
            let rest = entry_ids(tx, playlist)?;
            renumber_entries(tx, &rest)
        })
    }

    /// Moves the given entries (kept in their current relative order) so the
    /// first of them ends at `to_index` of the resulting list (clamped).
    pub fn move_playlist_entries(
        &self,
        playlist: PlaylistId,
        entries: &[i64],
        to_index: usize,
    ) -> Result<()> {
        self.write_tx(|tx| {
            require_playlist(tx, playlist)?;
            let all = entry_ids(tx, playlist)?;
            let picked: std::collections::HashSet<i64> = entries.iter().copied().collect();
            let moving: Vec<i64> = all.iter().copied().filter(|e| picked.contains(e)).collect();
            if moving.len() != picked.len() {
                return Err(LibraryError::Invalid("entry not in this playlist".into()));
            }
            let mut rest: Vec<i64> = all
                .iter()
                .copied()
                .filter(|e| !picked.contains(e))
                .collect();
            let at = to_index.min(rest.len());
            rest.splice(at..at, moving);
            renumber_entries(tx, &rest)?;
            tx.execute(
                "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
                params![playlist.0, now_ms()],
            )?;
            Ok(())
        })
    }

    /// Entries of a playlist in order, with track summaries.
    pub fn playlist_entries(&self, playlist: PlaylistId) -> Result<Vec<PlaylistEntry>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(&format!(
                "SELECT e.entry_id, e.position, {ROW_COLUMNS} FROM playlist_tracks e \
                 JOIN tracks t ON t.id = e.track_id \
                 WHERE e.playlist_id = ?1 ORDER BY e.position, e.entry_id"
            ))?;
            let rows = stmt
                .query_map([playlist.0], |r| {
                    Ok(PlaylistEntry {
                        entry_id: r.get(0)?,
                        position: r.get(1)?,
                        track: row_to_track_row_at(r, 2)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Track ids of a playlist in order.
    pub fn playlist_track_ids(&self, playlist: PlaylistId) -> Result<Vec<TrackId>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position, entry_id",
            )?;
            let rows = stmt
                .query_map([playlist.0], |r| r.get(0).map(TrackId))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }
}

fn renumber_entries(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "UPDATE playlist_tracks SET position = ?2 WHERE entry_id = ?1 AND position <> ?2",
    )?;
    for (i, id) in ids.iter().enumerate() {
        stmt.execute(params![id, i as i64])?;
    }
    Ok(())
}
