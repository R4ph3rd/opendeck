//! Tracks: CRUD, artwork and the collection view query.

use opendeck_contracts::ids::TrackId;
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

use crate::error::{LibraryError, Result};
use crate::library::{Library, not_found, now_ms};
use crate::types::{
    AnalysisStatus, NewTrack, SortColumn, Track, TrackMeta, TrackPage, TrackQuery, TrackRow,
    TrackTech,
};

/// Columns of [`TrackRow`], from `tracks t`.
pub(crate) const ROW_COLUMNS: &str = "t.id, t.path, t.title, t.artist, t.album, t.genre, t.year, \
     t.duration_secs, t.bitrate_kbps, t.bpm, t.musical_key, t.rating, t.color, t.date_added, \
     t.analysis_status, t.missing, t.artwork_id, t.comment";

pub(crate) fn row_to_track_row(r: &Row<'_>) -> rusqlite::Result<TrackRow> {
    row_to_track_row_at(r, 0)
}

/// Maps a [`ROW_COLUMNS`] selection that starts at column `o`.
pub(crate) fn row_to_track_row_at(r: &Row<'_>, o: usize) -> rusqlite::Result<TrackRow> {
    let status: String = r.get(o + 14)?;
    Ok(TrackRow {
        id: TrackId(r.get(o)?),
        path: r.get(o + 1)?,
        title: r.get(o + 2)?,
        artist: r.get(o + 3)?,
        album: r.get(o + 4)?,
        genre: r.get(o + 5)?,
        year: r.get(o + 6)?,
        duration_secs: r.get(o + 7)?,
        bitrate_kbps: r.get(o + 8)?,
        bpm: r.get(o + 9)?,
        key: r.get(o + 10)?,
        rating: r.get(o + 11)?,
        color: r.get(o + 12)?,
        date_added: r.get(o + 13)?,
        analysis_status: AnalysisStatus::parse(&status).unwrap_or_default(),
        missing: r.get::<_, i64>(o + 15)? != 0,
        artwork_id: r.get(o + 16)?,
        comment: r.get(o + 17)?,
    })
}

const TRACK_COLUMNS: &str = "id, path, file_size, file_mtime, content_hash, title, artist, album, \
     album_artist, remixer, label, composer, genre, year, track_number, comment, tag_bpm, tag_key, \
     duration_secs, sample_rate, channels, bitrate_kbps, codec, rating, color, play_count, \
     last_played_at, artwork_id, date_added, analysis_status, missing";

fn row_to_track(r: &Row<'_>) -> rusqlite::Result<Track> {
    let status: String = r.get(29)?;
    Ok(Track {
        id: TrackId(r.get(0)?),
        path: r.get(1)?,
        file_size: u64::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
        file_mtime: r.get(3)?,
        content_hash: r.get(4)?,
        meta: TrackMeta {
            title: r.get(5)?,
            artist: r.get(6)?,
            album: r.get(7)?,
            album_artist: r.get(8)?,
            remixer: r.get(9)?,
            label: r.get(10)?,
            composer: r.get(11)?,
            genre: r.get(12)?,
            year: r.get(13)?,
            track_number: r.get(14)?,
            comment: r.get(15)?,
            tag_bpm: r.get(16)?,
            tag_key: r.get(17)?,
        },
        tech: TrackTech {
            duration_secs: r.get(18)?,
            sample_rate: r.get(19)?,
            channels: r.get(20)?,
            bitrate_kbps: r.get(21)?,
            codec: r.get(22)?,
        },
        rating: r.get(23)?,
        color: r.get(24)?,
        play_count: r.get(25)?,
        last_played_at: r.get(26)?,
        artwork_id: r.get(27)?,
        date_added: r.get(28)?,
        analysis_status: AnalysisStatus::parse(&status).unwrap_or_default(),
        missing: r.get::<_, i64>(30)? != 0,
    })
}

/// Inserts a track on `conn`; shared by [`Library::insert_track`] and the importer.
pub(crate) fn insert_track_on(conn: &Connection, t: &NewTrack) -> Result<TrackId> {
    if t.rating > 5 {
        return Err(LibraryError::Invalid("rating must be 0 to 5".into()));
    }
    let size = i64::try_from(t.file_size)
        .map_err(|_| LibraryError::Invalid("file size out of range".into()))?;
    conn.execute(
        "INSERT INTO tracks (path, content_hash, file_size, file_mtime, title, artist, album, \
         album_artist, remixer, label, composer, genre, year, track_number, comment, tag_bpm, \
         tag_key, duration_secs, sample_rate, channels, bitrate_kbps, codec, rating, color, \
         artwork_id, date_added) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, \
         ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
        params![
            t.path,
            t.content_hash,
            size,
            t.file_mtime,
            t.meta.title,
            t.meta.artist,
            t.meta.album,
            t.meta.album_artist,
            t.meta.remixer,
            t.meta.label,
            t.meta.composer,
            t.meta.genre,
            t.meta.year,
            t.meta.track_number,
            t.meta.comment,
            t.meta.tag_bpm,
            t.meta.tag_key,
            t.tech.duration_secs,
            t.tech.sample_rate,
            t.tech.channels,
            t.tech.bitrate_kbps,
            t.tech.codec,
            t.rating,
            t.color,
            t.artwork_id,
            t.date_added.unwrap_or_else(now_ms),
        ],
    )?;
    Ok(TrackId(conn.last_insert_rowid()))
}

pub(crate) fn track_by_path_on(conn: &Connection, path: &str) -> Result<Option<TrackId>> {
    Ok(conn
        .query_row("SELECT id FROM tracks WHERE path = ?1", [path], |r| {
            r.get(0).map(TrackId)
        })
        .optional()?)
}

/// Tracks with the same size and fingerprint: `(id, path)`.
pub(crate) fn tracks_by_fingerprint_on(
    conn: &Connection,
    size: u64,
    hash: &[u8],
) -> Result<Vec<(TrackId, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, path FROM tracks WHERE file_size = ?1 AND content_hash = ?2 ORDER BY id",
    )?;
    let rows = stmt
        .query_map(params![size as i64, hash], |r| {
            Ok((TrackId(r.get(0)?), r.get(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Turns user text into an FTS5 query: every word becomes a quoted prefix term.
/// Returns `None` when the text has no searchable characters.
fn fts_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// How a sort column is read: the SQL expression, whether it compares
/// case-insensitively, and whether it can be NULL.
fn sort_spec(col: SortColumn) -> (&'static str, bool, bool) {
    match col {
        SortColumn::Title => ("t.title", true, true),
        SortColumn::Artist => ("t.artist", true, true),
        SortColumn::Album => ("t.album", true, true),
        SortColumn::Genre => ("t.genre", true, true),
        SortColumn::Duration => ("t.duration_secs", false, true),
        SortColumn::Bitrate => ("t.bitrate_kbps", false, true),
        SortColumn::Year => ("t.year", false, true),
        SortColumn::DateAdded => ("t.date_added", false, false),
        SortColumn::Bpm => ("t.bpm", false, true),
        // Camelot "10A": number first, then letter (see migration 0002).
        SortColumn::Key => ("t.key_sort", false, true),
        SortColumn::Rating => ("t.rating", false, false),
        SortColumn::AnalysisStatus => ("t.analysis_status", false, false),
    }
}

fn build_filter(q: &TrackQuery) -> (String, Vec<Value>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Value> = Vec::new();
    if let Some(m) = q.text.as_deref().and_then(fts_query) {
        clauses.push("t.id IN (SELECT rowid FROM tracks_fts WHERE tracks_fts MATCH ?)".into());
        args.push(Value::Text(m));
    }
    if let Some(v) = q.bpm_min {
        clauses.push("t.bpm >= ?".into());
        args.push(Value::Real(v));
    }
    if let Some(v) = q.bpm_max {
        clauses.push("t.bpm <= ?".into());
        args.push(Value::Real(v));
    }
    if !q.keys.is_empty() {
        let marks = vec!["?"; q.keys.len()].join(", ");
        clauses.push(format!("t.musical_key IN ({marks})"));
        args.extend(q.keys.iter().cloned().map(Value::Text));
    }
    if let Some(g) = &q.genre {
        clauses.push("t.genre = ? COLLATE NOCASE".into());
        args.push(Value::Text(g.clone()));
    }
    if let Some(r) = q.min_rating {
        clauses.push("t.rating >= ?".into());
        args.push(Value::Integer(i64::from(r)));
    }
    if let Some(s) = q.status {
        clauses.push("t.analysis_status = ?".into());
        args.push(Value::Text(s.as_str().into()));
    }
    if !q.include_missing {
        clauses.push("t.missing = 0".into());
    }
    let sql = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    (sql, args)
}

impl Library {
    /// Inserts a track. Fails when the path already exists.
    pub fn insert_track(&self, track: &NewTrack) -> Result<TrackId> {
        self.write(|c| insert_track_on(c, track))
    }

    /// Inserts many tracks in one transaction (bulk import, tests).
    pub fn insert_tracks(&self, tracks: &[NewTrack]) -> Result<Vec<TrackId>> {
        self.write_tx(|tx| tracks.iter().map(|t| insert_track_on(tx, t)).collect())
    }

    /// Full row for `id`.
    pub fn get_track(&self, id: TrackId) -> Result<Option<Track>> {
        self.read(|c| {
            Ok(c.query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM tracks WHERE id = ?1"),
                [id.0],
                row_to_track,
            )
            .optional()?)
        })
    }

    /// Track with this normalized path.
    pub fn track_by_path(&self, path: &str) -> Result<Option<TrackId>> {
        self.read(|c| track_by_path_on(c, path))
    }

    /// Lightweight rows for the given ids, in the order given (missing ids are skipped).
    pub fn track_rows(&self, ids: &[TrackId]) -> Result<Vec<TrackRow>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(&format!(
                "SELECT {ROW_COLUMNS} FROM tracks t WHERE t.id = ?1"
            ))?;
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(row) = stmt.query_row([id.0], row_to_track_row).optional()? {
                    out.push(row);
                }
            }
            Ok(out)
        })
    }

    /// Replaces the editable tag fields.
    pub fn update_track_meta(&self, id: TrackId, m: &TrackMeta) -> Result<()> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE tracks SET title = ?2, artist = ?3, album = ?4, album_artist = ?5, \
                 remixer = ?6, label = ?7, composer = ?8, genre = ?9, year = ?10, \
                 track_number = ?11, comment = ?12, tag_bpm = ?13, tag_key = ?14 WHERE id = ?1",
                params![
                    id.0,
                    m.title,
                    m.artist,
                    m.album,
                    m.album_artist,
                    m.remixer,
                    m.label,
                    m.composer,
                    m.genre,
                    m.year,
                    m.track_number,
                    m.comment,
                    m.tag_bpm,
                    m.tag_key
                ],
            )?;
            if n == 0 {
                Err(not_found("track"))
            } else {
                Ok(())
            }
        })
    }

    /// Sets the rating (0 to 5).
    pub fn set_rating(&self, id: TrackId, rating: u8) -> Result<()> {
        if rating > 5 {
            return Err(LibraryError::Invalid("rating must be 0 to 5".into()));
        }
        self.write(|c| {
            let n = c.execute(
                "UPDATE tracks SET rating = ?2 WHERE id = ?1",
                params![id.0, rating],
            )?;
            if n == 0 {
                Err(not_found("track"))
            } else {
                Ok(())
            }
        })
    }

    /// Sets or clears the colour label (`0xRRGGBB`).
    pub fn set_color(&self, id: TrackId, color: Option<u32>) -> Result<()> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE tracks SET color = ?2 WHERE id = ?1",
                params![id.0, color],
            )?;
            if n == 0 {
                Err(not_found("track"))
            } else {
                Ok(())
            }
        })
    }

    /// Counts a play: increments `play_count` and stamps `last_played_at`.
    pub fn mark_played(&self, id: TrackId) -> Result<()> {
        self.write(|c| {
            c.execute(
                "UPDATE tracks SET play_count = play_count + 1, last_played_at = ?2 WHERE id = ?1",
                params![id.0, now_ms()],
            )?;
            Ok(())
        })
    }

    /// Flags a track as missing or present.
    pub fn set_missing(&self, id: TrackId, missing: bool) -> Result<()> {
        self.write(|c| {
            c.execute(
                "UPDATE tracks SET missing = ?2 WHERE id = ?1",
                params![id.0, missing],
            )?;
            Ok(())
        })
    }

    /// Points a track at a new path (relink after a move); clears `missing`.
    pub fn relink_track(&self, id: TrackId, new_path: &str) -> Result<()> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE tracks SET path = ?2, missing = 0 WHERE id = ?1",
                params![id.0, new_path],
            )?;
            if n == 0 {
                Err(not_found("track"))
            } else {
                Ok(())
            }
        })
    }

    /// Deletes tracks; analysis, cues and playlist entries cascade, saved sets keep their snapshot.
    pub fn delete_tracks(&self, ids: &[TrackId]) -> Result<u64> {
        self.write_tx(|tx| {
            let mut n = 0;
            for id in ids {
                n += tx.execute("DELETE FROM tracks WHERE id = ?1", [id.0])? as u64;
            }
            Ok(n)
        })
    }

    /// Number of tracks.
    pub fn track_count(&self) -> Result<u64> {
        self.read(|c| {
            Ok(c.query_row("SELECT count(*) FROM tracks", [], |r| r.get::<_, i64>(0))? as u64)
        })
    }

    /// Sets the analysis status.
    pub fn set_analysis_status(&self, id: TrackId, status: AnalysisStatus) -> Result<()> {
        self.write(|c| {
            c.execute(
                "UPDATE tracks SET analysis_status = ?2 WHERE id = ?1",
                params![id.0, status.as_str()],
            )?;
            Ok(())
        })
    }

    /// Stores artwork bytes, deduped by BLAKE3 hash; returns the row id.
    pub fn store_artwork(&self, mime: &str, data: &[u8]) -> Result<i64> {
        self.write(|c| store_artwork_on(c, mime, data))
    }

    /// Artwork `(mime, bytes)`.
    pub fn artwork(&self, id: i64) -> Result<Option<(String, Vec<u8>)>> {
        self.read(|c| {
            Ok(
                c.query_row("SELECT mime, data FROM artwork WHERE id = ?1", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?,
            )
        })
    }

    /// Collection view: filter, sort and page the library.
    ///
    /// NULLs sort last in both directions. Each sort column has an index, so
    /// a page is an index walk plus one point lookup per row, however deep the
    /// offset. A nullable column is read in two parts, the non-NULL rows in
    /// index order, then the NULL rows by id, so both directions use the index.
    pub fn query_tracks(&self, q: &TrackQuery) -> Result<TrackPage> {
        let (filter, args) = build_filter(q);
        let (expr, nocase, nullable) = sort_spec(q.sort);
        let dir = if q.descending { "DESC" } else { "ASC" };
        let collate = if nocase { " COLLATE NOCASE" } else { "" };
        let and = if filter.is_empty() { " WHERE" } else { " AND" };
        self.read(|c| {
            // Matches, and how many of them have a value in the sort column.
            let (total, with_value): (i64, i64) = c.query_row(
                &format!("SELECT count(*), count({expr}) FROM tracks t{filter}"),
                params_from_iter(args.iter()),
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let total = u64::try_from(total).unwrap_or(0);
            let with_value = if nullable {
                u64::try_from(with_value).unwrap_or(0)
            } else {
                total
            };
            let mut rows: Vec<TrackRow> = Vec::new();
            let run = |extra: &str, order: &str, offset: u64, limit: u64| -> Result<Vec<TrackRow>> {
                let mut a = args.clone();
                a.push(Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
                a.push(Value::Integer(i64::try_from(offset).unwrap_or(i64::MAX)));
                let sql = format!(
                    "SELECT {ROW_COLUMNS} FROM tracks t{filter}{extra} ORDER BY {order} LIMIT ? OFFSET ?"
                );
                let mut stmt = c.prepare_cached(&sql)?;
                let got = stmt
                    .query_map(params_from_iter(a.iter()), row_to_track_row)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(got)
            };
            let offset = u64::from(q.offset);
            let limit = u64::from(q.limit);
            let ordered = format!("{expr}{collate} {dir}, t.id {dir}");
            if !nullable {
                rows = run("", &ordered, offset, limit)?;
            } else {
                if offset < with_value {
                    rows = run(&format!("{and} {expr} IS NOT NULL"), &ordered, offset, limit)?;
                }
                let have = rows.len() as u64;
                if have < limit {
                    let nulls_offset = offset.saturating_sub(with_value);
                    rows.extend(run(&format!("{and} {expr} IS NULL"), "t.id", nulls_offset, limit - have)?);
                }
            }
            Ok(TrackPage { rows, total })
        })
    }

    /// Distinct non-empty genres with track counts, most common first.
    pub fn genres(&self) -> Result<Vec<(String, u64)>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT genre, count(*) AS n FROM tracks WHERE genre IS NOT NULL AND genre <> '' \
                 GROUP BY genre COLLATE NOCASE ORDER BY n DESC, genre COLLATE NOCASE",
            )?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }
}

pub(crate) fn store_artwork_on(conn: &Connection, mime: &str, data: &[u8]) -> Result<i64> {
    let hash = blake3::hash(data);
    conn.execute(
        "INSERT OR IGNORE INTO artwork (hash, mime, data) VALUES (?1, ?2, ?3)",
        params![hash.as_bytes().as_slice(), mime, data],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM artwork WHERE hash = ?1",
        [hash.as_bytes().as_slice()],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_quotes_words_as_prefixes() {
        assert_eq!(fts_query("Daft pun").as_deref(), Some("\"Daft\"* \"pun\"*"));
        assert_eq!(fts_query("  \"-*( "), None);
        assert_eq!(fts_query("AC/DC").as_deref(), Some("\"AC\"* \"DC\"*"));
    }
}
