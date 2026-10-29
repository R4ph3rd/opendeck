//! Cues: hot, memory and loop, per track.

use opendeck_contracts::cue::{Cue, CueColor, CueKind, HotCueIndex};
use opendeck_contracts::ids::{CueId, TrackId};
use opendeck_contracts::units::Seconds;
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::error::{LibraryError, Result};
use crate::library::{Library, not_found, now_ms};

const CUE_COLUMNS: &str = "id, kind, hot_index, start_secs, end_secs, color, name";

fn row_to_cue(r: &Row<'_>) -> rusqlite::Result<Cue> {
    let kind: String = r.get(1)?;
    let hot: Option<u8> = r.get(2)?;
    let kind = match (kind.as_str(), hot.and_then(HotCueIndex::new)) {
        ("hot", Some(index)) => CueKind::Hot { index },
        ("memory", _) => CueKind::Memory,
        _ => CueKind::Loop,
    };
    let color: Option<u32> = r.get(5)?;
    Ok(Cue {
        id: Some(CueId(r.get(0)?)),
        kind,
        start: Seconds(r.get(3)?),
        end: r.get::<_, Option<f64>>(4)?.map(Seconds),
        color: color.map(CueColor::from_rgb24),
        name: r.get(6)?,
    })
}

fn kind_columns(kind: CueKind) -> (&'static str, Option<u8>) {
    match kind {
        CueKind::Hot { index } => ("hot", Some(index.index() as u8)),
        CueKind::Memory => ("memory", None),
        CueKind::Loop => ("loop", None),
    }
}

fn check(cue: &Cue) -> Result<()> {
    if cue.is_valid() {
        Ok(())
    } else {
        Err(LibraryError::Invalid(
            "cue needs a finite start >= 0, start < end, and an end for loops".into(),
        ))
    }
}

pub(crate) fn add_cue_on(conn: &Connection, track: TrackId, cue: &Cue) -> Result<CueId> {
    check(cue)?;
    let (kind, hot) = kind_columns(cue.kind);
    if let Some(slot) = hot {
        // A pad holds one cue; setting it again replaces the old one.
        conn.execute(
            "DELETE FROM cues WHERE track_id = ?1 AND kind = 'hot' AND hot_index = ?2",
            params![track.0, slot],
        )?;
    }
    conn.execute(
        "INSERT INTO cues (track_id, kind, hot_index, start_secs, end_secs, color, name, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            track.0,
            kind,
            hot,
            cue.start.0,
            cue.end.map(|e| e.0),
            cue.color.map(|c| c.to_rgb24()),
            cue.name,
            now_ms()
        ],
    )?;
    Ok(CueId(conn.last_insert_rowid()))
}

impl Library {
    /// Adds a cue to `track`. A hot cue replaces whatever is in its pad slot.
    pub fn add_cue(&self, track: TrackId, cue: &Cue) -> Result<CueId> {
        self.write_tx(|tx| add_cue_on(tx, track, cue))
    }

    /// Updates a stored cue (`cue.id` required): position, loop end, colour, name, kind.
    pub fn update_cue(&self, cue: &Cue) -> Result<()> {
        check(cue)?;
        let id = cue
            .id
            .ok_or_else(|| LibraryError::Invalid("cue has no id".into()))?;
        let (kind, hot) = kind_columns(cue.kind);
        self.write_tx(|tx| {
            if let Some(slot) = hot {
                // Free the slot if another cue of the same track holds it.
                tx.execute(
                    "DELETE FROM cues WHERE kind = 'hot' AND hot_index = ?2 AND id <> ?1 \
                     AND track_id = (SELECT track_id FROM cues WHERE id = ?1)",
                    params![id.0, slot],
                )?;
            }
            let n = tx.execute(
                "UPDATE cues SET kind = ?2, hot_index = ?3, start_secs = ?4, end_secs = ?5, \
                 color = ?6, name = ?7 WHERE id = ?1",
                params![
                    id.0,
                    kind,
                    hot,
                    cue.start.0,
                    cue.end.map(|e| e.0),
                    cue.color.map(|c| c.to_rgb24()),
                    cue.name
                ],
            )?;
            if n == 0 {
                Err(not_found("cue"))
            } else {
                Ok(())
            }
        })
    }

    /// Deletes a cue; returns whether it existed.
    pub fn delete_cue(&self, id: CueId) -> Result<bool> {
        self.write(|c| Ok(c.execute("DELETE FROM cues WHERE id = ?1", [id.0])? > 0))
    }

    /// One cue.
    pub fn get_cue(&self, id: CueId) -> Result<Option<Cue>> {
        self.read(|c| {
            Ok(c.query_row(
                &format!("SELECT {CUE_COLUMNS} FROM cues WHERE id = ?1"),
                [id.0],
                row_to_cue,
            )
            .optional()?)
        })
    }

    /// All cues of `track`, ordered by position.
    pub fn cues(&self, track: TrackId) -> Result<Vec<Cue>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(&format!(
                "SELECT {CUE_COLUMNS} FROM cues WHERE track_id = ?1 ORDER BY start_secs, id"
            ))?;
            let rows = stmt
                .query_map([track.0], row_to_cue)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Removes every cue of `track`.
    pub fn clear_cues(&self, track: TrackId) -> Result<u64> {
        self.write(|c| Ok(c.execute("DELETE FROM cues WHERE track_id = ?1", [track.0])? as u64))
    }
}
