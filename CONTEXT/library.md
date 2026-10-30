# library

Owner: library-dev.

## Purpose
SQLite collection: schema and migrations, track queries for the collection view, playlists and folders, saved sets, cues and analysis persistence, settings, FX presets, MIDI mapping overrides, and the import job queue (discover, dedupe, tags and artwork, decode, analyze, persist) with progress, cancel and resume. rekordbox XML import.

## Owned paths
- `crates/library/**` (migrations append-only)

## Depends on
- `opendeck-contracts`: ids, `Cue`, `BeatGrid` blob, `PeakLevel` blob, `MusicalKey`, `JobProgress` and friends, `AnalysisFeatures`, `FxPreset`.
- May depend on `opendeck-media` (tags, decode) and `opendeck-analysis` (analyzers) for the pipeline (ADR 0002).

## External crates
- In use: `rusqlite` 0.40 (bundled, includes FTS5), `rusqlite_migration` 2.6, `thiserror`, `blake3` (dedupe fingerprint, MIT/Apache-2.0/CC0), `crossbeam-channel` (progress channel, work queue), `quick-xml` 0.42 (rekordbox XML, MIT), `serde` and `serde_json` (settings, job options).
- Not used: `rayon`. The import pool is plain threads, so the cancel flag reaches every worker. Default size is cores minus one, capped at 4 (`DEFAULT_MAX_WORKERS`); `ImportConfig::workers` overrides it.

## Rules (ADR 0005)
- Migrations append-only; back up the DB file before migrating an older schema; refuse to downgrade.
- One writer connection on the library thread; readers on separate connections. Never block the UI thread or touch the audio thread.
- Import job state is written per item so a crash or quit resumes from pending items.
- Progress events at most ~4 per second per job.
- Schema v2 (`0002_playlist_order_fts.sql`): `playlist_tracks` has its own `entry_id` and no unique position (one `UPDATE` shifts any range; the same track may repeat); the folder guard fires on insert, on moving an entry into a folder, and on turning a playlist with tracks into a folder; playlists only nest in folders; FTS5 `tracks_fts` over title, artist, album, genre, comment; `bpm`, `musical_key`, `key_sort` are copied onto `tracks` by triggers, so each sort column has an index and a collection page is an index walk.
- Callers use `Library` (clone and share; every call is sync, never from the audio thread) and `import::ImportManager`. Nothing else touches SQL.
- A locked beat grid (`analysis.grid_locked`) survives re-analysis and rekordbox import. Analysis blobs are written by `save_analysis`; fields a partial result lacks keep their old value.
- Import resume point: `import_job_items.stage` is `dedupe` once a placeholder track row exists, `tags` once tags are written; items finish with their analysis write in one transaction. Cancelled jobs are terminal; paused jobs resume only on request; `resume_pending` resumes `queued` and `running` jobs at startup.

## Acceptance per milestone
- M0: schema v1 with in-memory tests. **Done in wave 0a.**
- M2: folder/files/M3U import with dedupe (path, then size + hash), tags and artwork, decode and peaks and loudness analysis, persist; pause, cancel and resume across restart (test kills the worker mid-job); collection query with sort, filter and text search over 50k rows in under 50 ms; playlists and folders CRUD with ordering. **Done in M3 (library crate): the whole list, with an injected `Analyzer`. Worst query on 50k rows is about 23 ms in a debug build (text prefix search sorted by BPM), most under 5 ms.**
- M3: analysis persistence (grid lock respected on re-analysis). **Done.** Re-analysis of one track or many (`reanalyze`), stale detection by analyzer id and version.
- M4: cues CRUD. **Done in the library** (hot, memory, loop; a hot pad holds one cue). app-core wiring is still to do.
- M7: rekordbox XML import (tracks, playlists, cues; documented export format only, positions in seconds); saved sets. **Done in M3 (library crate)**: tracks found on disk, `TEMPO` as a locked constant grid (first marker only, `Battito` moves the anchor to the downbeat), `POSITION_MARK` as hot, memory and loop cues, `PLAYLISTS/NODE` tree, missing files listed in the job report. Saved sets CRUD done.

## Open questions
- Path normalization across OSes (case, separators, network drives) and moved-file relinking.
- Artwork: store originals or downscaled thumbnails (size budget).
- Variable-tempo rekordbox tracks keep only the first `TEMPO` segment until contracts gets a tempo map (grid blob version 2).
- `TODO(verify)` against the Pioneer rekordbox XML Format List: `POSITION_MARK` type codes and `Num`, rating scale (0 to 255 step 51), `Battito`.
- Memory: a decode for analysis holds the whole file as stereo f32, about 210 MB per ten-minute track at 44.1 kHz, once per worker (so up to about 840 MB with the default 4 workers). Streamed or mono analysis would cut it.
- ADR 0005 needs a v2 addendum (architect): the schema changes above and the `Library` ownership of all SQL.
