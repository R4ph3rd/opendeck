# ADR 0005: Library database schema v1 and migration policy

- Status: accepted
- Date: 2026-10-29
- Deciders: architect

## Context

The library holds the collection, analysis results (including waveform peaks), cues, playlists, saved sets, resumable import jobs, settings, FX presets and MIDI mapping overrides. It must survive app updates without data loss and never block the audio or UI threads.

## Decision

### Engine and access

- SQLite, bundled through `rusqlite` **0.40** (not 0.41: `rusqlite_migration` 2.6 requires `^0.40`, and two `rusqlite` versions cannot both link the bundled SQLite). Revisit when `rusqlite_migration` supports 0.41.
- On every open: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout=5s`. These are connection pragmas, so they live in `library::db::open`, not in migrations.
- One writer connection on the library's own thread; readers use separate connections (WAL allows concurrent reads). No connection is shared with the audio thread.

### Schema v1

`crates/library/migrations/0001_init.sql`. Conventions: times are Unix milliseconds; positions are seconds (`REAL`) from the first decoded frame, independent of sample rate; colours are `0xRRGGBB`; enum columns store the snake_case serde spelling of the matching contracts enum, guarded by `CHECK`.

| Table | Purpose | Notes |
|---|---|---|
| `tracks` | One row per file | `path` unique; `content_hash` (BLAKE3 of size + first and last 1 MiB) with `file_size` is the dedupe key; tag fields; `rating` 0-5; `analysis_status`; `missing` flag |
| `artwork` | Embedded images | Deduped by BLAKE3 `hash`; tracks reference it |
| `analysis` | One row per analysed track | `bpm`, `musical_key` (Camelot), `beat_grid` blob (version byte first, `contracts::beatgrid`), loudness, analyzer id and version, `grid_locked` |
| `analysis_peaks` | Peaks per zoom level | `(track_id, level)`, `format_version`, `frames_per_point`, blob in `contracts::peaks` layout (sent to the UI unchanged) |
| `cues` | Hot cues, memory cues, saved loops | `kind` hot/memory/loop; `hot_index` only and always for hot; one hot cue per slot (partial unique index); loops require `end_secs` |
| `playlists`, `playlist_tracks` | Tree of folders and playlists; ordered entries | Folders cannot hold tracks (trigger); entries ordered by `position` |
| `saved_sets`, `saved_set_tracks` | Played history of a session plus a JSON session snapshot | Entries keep a title/artist snapshot and survive track deletion |
| `import_jobs`, `import_job_items` | Resumable import queue | Item `status` and last completed `stage` are the resume point |
| `settings` | Key/value, JSON values | e.g. key `audio` = `AudioSettings` |
| `fx_presets` | Effect presets | Keyed by `EffectKey` (provider, effect id) and name; params as JSON |
| `midi_mapping_overrides` | User changes over shipped mappings | Per `(mapping_id, control_id)`; records the base mapping version |

### Migration policy

- `rusqlite_migration` applies embedded SQL files in order and tracks progress in `PRAGMA user_version` (no extra table).
- Files are `NNNN_snake_name.sql`, append-only. A released migration is never edited; a fix is a new migration.
- Each migration runs in a transaction. Destructive changes (drop column, change type) use the SQLite table-rebuild pattern inside the migration and keep data.
- Before migrating a database whose `user_version` is lower than the latest, the library copies the file to `library.sqlite3.bak-v<old>`.
- A database with a `user_version` higher than the app knows is opened read-only with an error shown; it is never downgraded.
- Every migration has an in-memory test (`migrations().validate()`, plus targeted constraint tests).

### Blob versioning

Beat grid and peak blobs carry their own format version, so their layout can evolve without a schema migration. Readers reject unknown versions and schedule re-analysis.

## Consequences

- Library code owns all SQL; other crates see contracts types only.
- Peaks are stored in exactly the wire format, so serving them is a single `SELECT` and a byte copy.
- Pinning `rusqlite` 0.40 is a deviation from ADR 0001 (which listed 0.41).

## Addendum: schema v2 (M3, 2026-10-30)

`crates/library/migrations/0002_playlist_order_fts.sql`. Status: accepted.

- `playlist_tracks` gets its own `entry_id` primary key. `position` is no longer unique; order is `(position, entry_id)`. One `UPDATE` shifts any range, and a track may appear twice in a playlist.
- Folder guard: tracks cannot be inserted into or moved (`UPDATE`) into a folder, and a playlist that holds tracks cannot become a folder.
- Only folders contain playlists, on insert and on update; a folder with children cannot become a playlist.
- FTS5 table `tracks_fts` over title, artist, album, genre and comment, kept in sync by triggers.
- `bpm`, `musical_key` and `key_sort` are denormalised onto `tracks` by triggers on `analysis` (never written by callers), with one index per sort column, so a collection page is an index walk.
- Upgrade: the file is checkpointed and copied to `<file>.bak-v<old>` before migrating, as above. A newer schema is refused with an error and left untouched (stricter than the read-only open described above).
- `Library` owns all SQL; callers use its API and `import::ImportManager`.
