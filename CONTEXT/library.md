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
- In use: `rusqlite` 0.40 (bundled), `rusqlite_migration` 2.6, `thiserror`.
- Add when used: `blake3` (dedupe hash: size + first and last 1 MiB), `crossbeam-channel` (job queue), `rayon` (analysis pool, cores - 1), `quick-xml` (rekordbox XML), `serde_json` (settings, presets).

## Rules (ADR 0005)
- Migrations append-only; back up the DB file before migrating an older schema; refuse to downgrade.
- One writer connection on the library thread; readers on separate connections. Never block the UI thread or touch the audio thread.
- Import job state is written per item so a crash or quit resumes from pending items.
- Progress events at most ~4 per second per job.

## Acceptance per milestone
- M0: schema v1 with in-memory tests. **Done in wave 0a.**
- M2: folder/files/M3U import with dedupe (path, then size + hash), tags and artwork, decode and peaks and loudness analysis, persist; pause, cancel and resume across restart (test kills the worker mid-job); collection query with sort, filter and text search over 50k rows in under 50 ms; playlists and folders CRUD with ordering.
- M3: analysis persistence (grid lock respected on re-analysis).
- M4: cues CRUD.
- M7: rekordbox XML import (tracks, playlists, cues; documented export format only, positions in seconds); saved sets.

## Open questions
- Path normalization across OSes (case, separators, network drives) and moved-file relinking.
- Artwork: store originals or downscaled thumbnails (size budget).
