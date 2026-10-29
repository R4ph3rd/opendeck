-- OpenDeck library schema v1 (ADR 0005).
-- Conventions: times are Unix milliseconds (INTEGER); positions are seconds
-- (REAL) from the first decoded frame; colours are 0xRRGGBB (INTEGER);
-- enum columns hold the snake_case serde spelling of the contracts enum.
-- Connection pragmas (WAL, foreign_keys, synchronous) are set on open, not here.

CREATE TABLE artwork (
    id          INTEGER PRIMARY KEY,
    hash        BLOB    NOT NULL UNIQUE,          -- BLAKE3 of the image bytes; dedupes shared album art
    mime        TEXT    NOT NULL,
    width       INTEGER,
    height      INTEGER,
    data        BLOB    NOT NULL                  -- embedded image bytes as read from the file
);

CREATE TABLE tracks (
    id              INTEGER PRIMARY KEY,
    path            TEXT    NOT NULL UNIQUE,       -- absolute, normalized
    content_hash    BLOB,                          -- BLAKE3 of size + first and last 1 MiB (ADR 0001); NULL until hashed
    file_size       INTEGER NOT NULL CHECK (file_size >= 0),
    file_mtime      INTEGER NOT NULL,
    title           TEXT,
    artist          TEXT,
    album           TEXT,
    album_artist    TEXT,
    remixer         TEXT,
    label           TEXT,
    composer        TEXT,
    genre           TEXT,
    year            INTEGER,
    track_number    INTEGER,
    comment         TEXT,
    tag_bpm         REAL,                          -- BPM from tags, informational; analysis.bpm is authoritative
    tag_key         TEXT,                          -- key from tags, as written
    duration_secs   REAL CHECK (duration_secs IS NULL OR duration_secs >= 0),
    sample_rate     INTEGER,
    channels        INTEGER,
    bitrate_kbps    INTEGER,
    codec           TEXT,
    rating          INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
    color           INTEGER,
    play_count      INTEGER NOT NULL DEFAULT 0,
    last_played_at  INTEGER,
    artwork_id      INTEGER REFERENCES artwork(id) ON DELETE SET NULL,
    date_added      INTEGER NOT NULL,
    analysis_status TEXT    NOT NULL DEFAULT 'pending'
                    CHECK (analysis_status IN ('pending', 'running', 'done', 'failed', 'stale')),
    missing         INTEGER NOT NULL DEFAULT 0 CHECK (missing IN (0, 1))
);

CREATE INDEX tracks_dedupe        ON tracks(file_size, content_hash);
CREATE INDEX tracks_artist        ON tracks(artist COLLATE NOCASE);
CREATE INDEX tracks_title         ON tracks(title COLLATE NOCASE);
CREATE INDEX tracks_genre         ON tracks(genre COLLATE NOCASE);
CREATE INDEX tracks_date_added    ON tracks(date_added);
CREATE INDEX tracks_analysis      ON tracks(analysis_status);
CREATE INDEX tracks_artwork       ON tracks(artwork_id);

-- One row per analysed track. Re-analysis replaces the row unless the user locked the grid.
CREATE TABLE analysis (
    track_id          INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    analyzer          TEXT    NOT NULL,            -- Analyzer::id of the tempo/key stage
    analyzer_version  INTEGER NOT NULL,
    analyzed_at       INTEGER NOT NULL,
    sample_rate       INTEGER NOT NULL,            -- rate of the decoded audio that was analysed
    bpm               REAL CHECK (bpm IS NULL OR bpm > 0),
    musical_key       TEXT,                        -- Camelot, e.g. '8A'
    beat_grid         BLOB,                        -- contracts::beatgrid blob; first byte is the format version
    grid_locked       INTEGER NOT NULL DEFAULT 0 CHECK (grid_locked IN (0, 1)),
    loudness_lufs     REAL,
    true_peak_dbtp    REAL
);

-- Waveform peaks per zoom level (level 0 = finest), contracts::peaks blob layout.
CREATE TABLE analysis_peaks (
    track_id          INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    level             INTEGER NOT NULL CHECK (level >= 0),
    format_version    INTEGER NOT NULL,
    frames_per_point  INTEGER NOT NULL CHECK (frames_per_point > 0),
    data              BLOB    NOT NULL,
    PRIMARY KEY (track_id, level)
) WITHOUT ROWID;

CREATE TABLE cues (
    id          INTEGER PRIMARY KEY,
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL CHECK (kind IN ('hot', 'memory', 'loop')),
    hot_index   INTEGER CHECK (hot_index BETWEEN 0 AND 7),
    start_secs  REAL    NOT NULL CHECK (start_secs >= 0),
    end_secs    REAL,
    color       INTEGER,
    name        TEXT    NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    CHECK ((kind = 'hot') = (hot_index IS NOT NULL)),
    CHECK (end_secs IS NULL OR end_secs > start_secs),
    CHECK (kind <> 'loop' OR end_secs IS NOT NULL)
);

CREATE UNIQUE INDEX cues_hot_slot ON cues(track_id, hot_index) WHERE kind = 'hot';
CREATE INDEX cues_track ON cues(track_id, start_secs);

-- Playlists and folders form a tree; siblings are ordered by position.
CREATE TABLE playlists (
    id          INTEGER PRIMARY KEY,
    parent_id   INTEGER REFERENCES playlists(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL CHECK (kind IN ('folder', 'playlist')),
    name        TEXT    NOT NULL,
    position    INTEGER NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX playlists_parent ON playlists(parent_id, position);

CREATE TABLE playlist_tracks (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    added_at    INTEGER NOT NULL,
    PRIMARY KEY (playlist_id, position)
) WITHOUT ROWID;

CREATE INDEX playlist_tracks_track ON playlist_tracks(track_id);

-- Folders hold playlists, never tracks.
CREATE TRIGGER playlist_tracks_not_in_folder
BEFORE INSERT ON playlist_tracks
WHEN (SELECT kind FROM playlists WHERE id = NEW.playlist_id) = 'folder'
BEGIN
    SELECT RAISE(ABORT, 'tracks cannot be added to a playlist folder');
END;

-- A saved set: the tracks played in a session, in order, plus a session snapshot.
CREATE TABLE saved_sets (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL,
    notes         TEXT    NOT NULL DEFAULT '',
    session_state TEXT,                            -- JSON: deck, mixer and FX state at save time
    started_at    INTEGER,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE saved_set_tracks (
    set_id      INTEGER NOT NULL REFERENCES saved_sets(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    track_id    INTEGER REFERENCES tracks(id) ON DELETE SET NULL,  -- history survives track removal
    title       TEXT,                              -- snapshot for display if the track is gone
    artist      TEXT,
    deck        INTEGER CHECK (deck IS NULL OR deck BETWEEN 0 AND 3),
    played_at   INTEGER,
    PRIMARY KEY (set_id, position)
) WITHOUT ROWID;

CREATE INDEX saved_set_tracks_track ON saved_set_tracks(track_id);

-- Import and analysis jobs. Item rows are the resume point after a restart.
CREATE TABLE import_jobs (
    id                  INTEGER PRIMARY KEY,
    kind                TEXT    NOT NULL CHECK (kind IN
                        ('import_folder', 'import_files', 'import_playlist', 'import_rekordbox_xml', 'reanalyze')),
    state               TEXT    NOT NULL DEFAULT 'queued' CHECK (state IN
                        ('queued', 'running', 'paused', 'cancelled', 'completed', 'failed')),
    source              TEXT,                      -- folder, playlist or XML path
    options             TEXT    NOT NULL DEFAULT '{}',  -- JSON: requested analysis features, target playlist rules
    target_playlist_id  INTEGER REFERENCES playlists(id) ON DELETE SET NULL,
    error               TEXT,
    created_at          INTEGER NOT NULL,
    started_at          INTEGER,
    finished_at         INTEGER
);

CREATE INDEX import_jobs_state ON import_jobs(state);

CREATE TABLE import_job_items (
    id          INTEGER PRIMARY KEY,
    job_id      INTEGER NOT NULL REFERENCES import_jobs(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,                  -- source order (playlist order for playlist imports)
    path        TEXT    NOT NULL,
    status      TEXT    NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'done', 'duplicate', 'failed')),
    stage       TEXT CHECK (stage IS NULL OR stage IN
                ('discover', 'dedupe', 'tags', 'decode', 'analyze', 'persist')),  -- last completed stage
    track_id    INTEGER REFERENCES tracks(id) ON DELETE SET NULL,
    error       TEXT,
    updated_at  INTEGER,
    UNIQUE (job_id, path)
);

CREATE INDEX import_job_items_status ON import_job_items(job_id, status, position);

-- Key/value settings; values are JSON (e.g. key 'audio' holds contracts::settings::AudioSettings).
CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT    NOT NULL,
    updated_at  INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE fx_presets (
    id              INTEGER PRIMARY KEY,
    provider        TEXT    NOT NULL,              -- EffectKey.provider
    effect_id       TEXT    NOT NULL,              -- EffectKey.id
    name            TEXT    NOT NULL,
    params          TEXT    NOT NULL,              -- JSON [[param_id, normalized], ...]
    wet             REAL    NOT NULL DEFAULT 1.0 CHECK (wet BETWEEN 0 AND 1),
    beat_division   REAL    NOT NULL DEFAULT 1.0 CHECK (beat_division > 0),
    is_factory      INTEGER NOT NULL DEFAULT 0 CHECK (is_factory IN (0, 1)),
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    UNIQUE (provider, effect_id, name)
);

-- User changes on top of a shipped JSON mapping (ADR 0007). The base file stays read-only.
CREATE TABLE midi_mapping_overrides (
    id              INTEGER PRIMARY KEY,
    mapping_id      TEXT    NOT NULL,              -- mapping file "id"
    mapping_version INTEGER NOT NULL,              -- base mapping "version" the override was made against
    control_id      TEXT    NOT NULL,              -- control "id" inside the mapping
    binding         TEXT    NOT NULL,              -- JSON binding object per mapping.schema.json, or 'null' to unbind
    updated_at      INTEGER NOT NULL,
    UNIQUE (mapping_id, control_id)
);
