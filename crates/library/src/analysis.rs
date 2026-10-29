//! Analysis persistence: tempo, key, loudness, beat grid and peaks.

use opendeck_contracts::analysis::{AnalysisResult, Loudness};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::key::MusicalKey;
use opendeck_contracts::peaks::PEAK_FORMAT_VERSION;
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{LibraryError, Result};
use crate::library::{Library, not_found, now_ms};
use crate::types::{AnalysisStatus, AnalyzerStamp, PeakLevelInfo, StoredAnalysis};

/// What [`Library::save_analysis`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveOutcome {
    /// A locked grid was kept instead of the analysed one.
    pub grid_kept: bool,
}

/// Writes an analysis result on `conn` (also used inside the importer's transaction).
///
/// A re-analysis replaces the row, except that fields the new result does not
/// provide keep their old value, and a locked grid (and its BPM) is kept.
pub(crate) fn save_analysis_on(
    conn: &Connection,
    track: TrackId,
    stamp: &AnalyzerStamp,
    result: &AnalysisResult,
) -> Result<SaveOutcome> {
    let existing: Option<(bool, Option<f64>, Option<Vec<u8>>)> = conn
        .query_row(
            "SELECT grid_locked, bpm, beat_grid FROM analysis WHERE track_id = ?1",
            [track.0],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let locked = existing.as_ref().is_some_and(|e| e.0);

    let (bpm, grid_blob) = if locked {
        let e = existing.as_ref().expect("locked implies existing");
        (e.1, e.2.clone())
    } else {
        let grid_bpm = result.beat_grid.as_ref().map(|g| g.bpm().0);
        (
            result
                .bpm
                .map(|b| b.0)
                .or(grid_bpm)
                .or(existing.as_ref().and_then(|e| e.1)),
            result
                .beat_grid
                .as_ref()
                .map(BeatGrid::to_blob)
                .or_else(|| existing.as_ref().and_then(|e| e.2.clone())),
        )
    };
    let key = result.key.map(|k| k.camelot());
    let sr = i64::from(stamp.sample_rate);
    // `analysis.sample_rate` is NOT NULL; a partial result still records the rate it was made at.
    conn.execute(
        "INSERT INTO analysis (track_id, analyzer, analyzer_version, analyzed_at, sample_rate, bpm, \
         musical_key, beat_grid, loudness_lufs, true_peak_dbtp) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(track_id) DO UPDATE SET analyzer = excluded.analyzer, \
           analyzer_version = excluded.analyzer_version, analyzed_at = excluded.analyzed_at, \
           sample_rate = excluded.sample_rate, bpm = excluded.bpm, \
           musical_key = COALESCE(excluded.musical_key, musical_key), \
           beat_grid = excluded.beat_grid, \
           loudness_lufs = COALESCE(excluded.loudness_lufs, loudness_lufs), \
           true_peak_dbtp = COALESCE(excluded.true_peak_dbtp, true_peak_dbtp)",
        params![
            track.0,
            stamp.analyzer,
            stamp.version,
            now_ms(),
            sr,
            bpm,
            key,
            grid_blob,
            result.loudness.map(|l| l.integrated_lufs),
            result.loudness.map(|l| l.true_peak_dbtp),
        ],
    )?;
    if !result.peaks.is_empty() {
        conn.execute("DELETE FROM analysis_peaks WHERE track_id = ?1", [track.0])?;
        for (level, peaks) in result.peaks.iter().enumerate() {
            conn.execute(
                "INSERT INTO analysis_peaks (track_id, level, format_version, frames_per_point, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    track.0,
                    level as i64,
                    i64::from(PEAK_FORMAT_VERSION),
                    peaks.frames_per_point,
                    peaks.encode()
                ],
            )?;
        }
    }
    conn.execute(
        "UPDATE tracks SET analysis_status = 'done' WHERE id = ?1",
        [track.0],
    )?;
    Ok(SaveOutcome { grid_kept: locked })
}

/// Writes a beat grid on `conn`. With `respect_lock`, a locked grid is left alone;
/// returns whether the grid was written.
pub(crate) fn set_beat_grid_on(
    conn: &Connection,
    track: TrackId,
    grid: &BeatGrid,
    locked: bool,
    respect_lock: bool,
) -> Result<bool> {
    let exists: bool = conn
        .query_row("SELECT 1 FROM tracks WHERE id = ?1", [track.0], |_| {
            Ok(true)
        })
        .optional()?
        .unwrap_or(false);
    if !exists {
        return Err(not_found("track"));
    }
    let guard = if respect_lock {
        " WHERE analysis.grid_locked = 0"
    } else {
        ""
    };
    let n = conn.execute(
        &format!(
            "INSERT INTO analysis (track_id, analyzer, analyzer_version, analyzed_at, sample_rate, \
             bpm, beat_grid, grid_locked) VALUES (?1, 'manual', 0, ?2, 0, ?3, ?4, ?5) \
             ON CONFLICT(track_id) DO UPDATE SET bpm = excluded.bpm, beat_grid = excluded.beat_grid, \
               grid_locked = excluded.grid_locked{guard}"
        ),
        params![track.0, now_ms(), grid.bpm().0, grid.to_blob(), locked],
    )?;
    Ok(n > 0)
}

impl Library {
    /// Stores an analysis result for `track` and marks it `done`. See
    /// [`SaveOutcome`]: a locked grid survives re-analysis.
    pub fn save_analysis(
        &self,
        track: TrackId,
        stamp: &AnalyzerStamp,
        result: &AnalysisResult,
    ) -> Result<SaveOutcome> {
        self.write_tx(|tx| save_analysis_on(tx, track, stamp, result))
    }

    /// Stored analysis of `track`, `None` when never analysed.
    pub fn get_analysis(&self, track: TrackId) -> Result<Option<StoredAnalysis>> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT analyzer, analyzer_version, analyzed_at, sample_rate, bpm, musical_key, \
                 beat_grid, grid_locked, loudness_lufs, true_peak_dbtp \
                 FROM analysis WHERE track_id = ?1",
                [track.0],
                |r| {
                    let key: Option<String> = r.get(5)?;
                    let blob: Option<Vec<u8>> = r.get(6)?;
                    let lufs: Option<f64> = r.get(8)?;
                    let tp: Option<f64> = r.get(9)?;
                    Ok(StoredAnalysis {
                        analyzer: r.get(0)?,
                        analyzer_version: r.get(1)?,
                        analyzed_at: r.get(2)?,
                        sample_rate: r.get(3)?,
                        bpm: r.get(4)?,
                        key: key.and_then(|k| MusicalKey::parse(&k).ok()),
                        // An unknown blob version reads as "no grid" (ADR 0005: schedule re-analysis).
                        beat_grid: blob.and_then(|b| BeatGrid::from_blob(&b).ok()),
                        grid_locked: r.get::<_, i64>(7)? != 0,
                        loudness: lufs
                            .zip(tp)
                            .map(|(integrated_lufs, true_peak_dbtp)| Loudness {
                                integrated_lufs,
                                true_peak_dbtp,
                            }),
                    })
                },
            )
            .optional()?)
        })
    }

    /// Sets the beat grid by hand (or from an import) and optionally locks it.
    /// Creates a minimal analysis row when the track has none.
    pub fn set_beat_grid(&self, track: TrackId, grid: &BeatGrid, locked: bool) -> Result<()> {
        self.write_tx(|tx| set_beat_grid_on(tx, track, grid, locked, false).map(|_| ()))
    }

    /// Locks or unlocks the grid against re-analysis.
    pub fn set_grid_locked(&self, track: TrackId, locked: bool) -> Result<()> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE analysis SET grid_locked = ?2 WHERE track_id = ?1",
                params![track.0, locked],
            )?;
            if n == 0 {
                Err(not_found("analysis"))
            } else {
                Ok(())
            }
        })
    }

    /// Peak levels stored for `track`, finest first.
    pub fn peak_levels(&self, track: TrackId) -> Result<Vec<PeakLevelInfo>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT level, frames_per_point, format_version, length(data) \
                 FROM analysis_peaks WHERE track_id = ?1 ORDER BY level",
            )?;
            let rows = stmt
                .query_map([track.0], |r| {
                    Ok(PeakLevelInfo {
                        level: r.get(0)?,
                        frames_per_point: r.get(1)?,
                        format_version: r.get(2)?,
                        byte_len: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Peak blob of one zoom level, in the `contracts::peaks` wire layout.
    pub fn get_peaks(&self, track: TrackId, level: u32) -> Result<Option<Vec<u8>>> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT data FROM analysis_peaks WHERE track_id = ?1 AND level = ?2",
                params![track.0, level],
                |r| r.get(0),
            )
            .optional()?)
        })
    }

    /// Tracks never analysed by `analyzer` at `version` or newer: no analysis,
    /// another analyzer, or an older version. Candidates for a re-analysis job.
    pub fn tracks_needing_analysis(&self, analyzer: &str, version: u32) -> Result<Vec<TrackId>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT t.id FROM tracks t LEFT JOIN analysis a ON a.track_id = t.id \
                 WHERE t.missing = 0 AND (a.track_id IS NULL OR a.analyzer <> ?1 OR a.analyzer_version < ?2) \
                 ORDER BY t.id",
            )?;
            let rows = stmt
                .query_map(params![analyzer, version], |r| r.get(0).map(TrackId))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Marks analysed tracks whose analyzer or version differs as `stale`; returns how many.
    pub fn mark_stale(&self, analyzer: &str, version: u32) -> Result<u64> {
        self.write(|c| {
            let n = c.execute(
                "UPDATE tracks SET analysis_status = 'stale' WHERE analysis_status = 'done' AND id IN \
                 (SELECT track_id FROM analysis WHERE analyzer <> ?1 OR analyzer_version < ?2)",
                params![analyzer, version],
            )?;
            Ok(n as u64)
        })
    }

    /// Status of a track, `None` if it does not exist.
    pub fn analysis_status(&self, track: TrackId) -> Result<Option<AnalysisStatus>> {
        self.read(|c| {
            let s: Option<String> = c
                .query_row(
                    "SELECT analysis_status FROM tracks WHERE id = ?1",
                    [track.0],
                    |r| r.get(0),
                )
                .optional()?;
            s.map(|s| {
                AnalysisStatus::parse(&s)
                    .ok_or_else(|| LibraryError::Corrupt(format!("status {s}")))
            })
            .transpose()
        })
    }
}
