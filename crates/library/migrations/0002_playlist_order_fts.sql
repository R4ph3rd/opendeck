-- OpenDeck library schema v2 (ADR 0005, append-only: 0001 shipped in v0.1.0 and v0.2.0).
--
-- 1. Playlist reordering. 0001 made (playlist_id, position) the primary key, so a
--    bulk shift (insert at the top, move a block) violated the constraint midway
--    unless every statement ran in a perfect order. Positions stay dense integers
--    but are no longer unique: entries get their own row id (which also allows the
--    same track twice in one playlist, as in a DJ set) and ordering is
--    (position, entry_id). One UPDATE can shift any range.
-- 2. The folder guard also fires when an entry is moved to a folder and when a
--    playlist that holds tracks is turned into a folder. Playlists can only be
--    children of folders.
-- 3. FTS5 text search over title, artist, album, genre and comment.
-- 4. Collection view speed. BPM and key are copied from `analysis` onto `tracks`
--    (kept in sync by triggers, never written by callers), so every sort and
--    filter column lives in one table and each sort column has an index. A page
--    of a 50k-track view is then an index walk, not a join plus a sort, even at
--    a deep offset.

DROP TRIGGER playlist_tracks_not_in_folder;

CREATE TABLE playlist_tracks_v2 (
    entry_id    INTEGER PRIMARY KEY,
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    added_at    INTEGER NOT NULL
);

INSERT INTO playlist_tracks_v2 (playlist_id, position, track_id, added_at)
    SELECT playlist_id, position, track_id, added_at
    FROM playlist_tracks
    ORDER BY playlist_id, position;

DROP TABLE playlist_tracks;
ALTER TABLE playlist_tracks_v2 RENAME TO playlist_tracks;

CREATE INDEX playlist_tracks_order ON playlist_tracks(playlist_id, position, entry_id);
CREATE INDEX playlist_tracks_track ON playlist_tracks(track_id);

CREATE TRIGGER playlist_tracks_not_in_folder_insert
BEFORE INSERT ON playlist_tracks
WHEN (SELECT kind FROM playlists WHERE id = NEW.playlist_id) = 'folder'
BEGIN
    SELECT RAISE(ABORT, 'tracks cannot be added to a playlist folder');
END;

CREATE TRIGGER playlist_tracks_not_in_folder_update
BEFORE UPDATE OF playlist_id ON playlist_tracks
WHEN (SELECT kind FROM playlists WHERE id = NEW.playlist_id) = 'folder'
BEGIN
    SELECT RAISE(ABORT, 'tracks cannot be moved into a playlist folder');
END;

CREATE TRIGGER playlists_folder_holds_no_tracks
BEFORE UPDATE OF kind ON playlists
WHEN NEW.kind = 'folder'
 AND EXISTS (SELECT 1 FROM playlist_tracks WHERE playlist_id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'a playlist with tracks cannot become a folder');
END;

CREATE TRIGGER playlists_parent_is_folder_insert
BEFORE INSERT ON playlists
WHEN NEW.parent_id IS NOT NULL
 AND (SELECT kind FROM playlists WHERE id = NEW.parent_id) <> 'folder'
BEGIN
    SELECT RAISE(ABORT, 'only a folder can contain playlists');
END;

CREATE TRIGGER playlists_parent_is_folder_update
BEFORE UPDATE OF parent_id ON playlists
WHEN NEW.parent_id IS NOT NULL
 AND (SELECT kind FROM playlists WHERE id = NEW.parent_id) <> 'folder'
BEGIN
    SELECT RAISE(ABORT, 'only a folder can contain playlists');
END;

CREATE TRIGGER playlists_folder_kind_keeps_children
BEFORE UPDATE OF kind ON playlists
WHEN NEW.kind = 'playlist'
 AND EXISTS (SELECT 1 FROM playlists WHERE parent_id = NEW.id)
BEGIN
    SELECT RAISE(ABORT, 'a folder with children cannot become a playlist');
END;

-- Denormalised from `analysis` (see 4 above). `key_sort` orders Camelot keys
-- by number, then A before B ('10A' after '9B').
ALTER TABLE tracks ADD COLUMN bpm REAL;
ALTER TABLE tracks ADD COLUMN musical_key TEXT;
ALTER TABLE tracks ADD COLUMN key_sort INTEGER;

UPDATE tracks SET
    bpm         = (SELECT a.bpm FROM analysis a WHERE a.track_id = tracks.id),
    musical_key = (SELECT a.musical_key FROM analysis a WHERE a.track_id = tracks.id),
    key_sort    = (SELECT CASE WHEN a.musical_key IS NULL THEN NULL
                          ELSE CAST(a.musical_key AS INTEGER) * 2 + (substr(a.musical_key, -1) = 'B') END
                   FROM analysis a WHERE a.track_id = tracks.id);

CREATE TRIGGER analysis_sync_insert AFTER INSERT ON analysis BEGIN
    UPDATE tracks SET
        bpm = NEW.bpm,
        musical_key = NEW.musical_key,
        key_sort = CASE WHEN NEW.musical_key IS NULL THEN NULL
                        ELSE CAST(NEW.musical_key AS INTEGER) * 2 + (substr(NEW.musical_key, -1) = 'B') END
    WHERE id = NEW.track_id;
END;

CREATE TRIGGER analysis_sync_update AFTER UPDATE OF bpm, musical_key ON analysis BEGIN
    UPDATE tracks SET
        bpm = NEW.bpm,
        musical_key = NEW.musical_key,
        key_sort = CASE WHEN NEW.musical_key IS NULL THEN NULL
                        ELSE CAST(NEW.musical_key AS INTEGER) * 2 + (substr(NEW.musical_key, -1) = 'B') END
    WHERE id = NEW.track_id;
END;

CREATE TRIGGER analysis_sync_delete AFTER DELETE ON analysis BEGIN
    UPDATE tracks SET bpm = NULL, musical_key = NULL, key_sort = NULL WHERE id = OLD.track_id;
END;

-- One index per collection view sort and filter column.
CREATE INDEX tracks_album    ON tracks(album COLLATE NOCASE);
CREATE INDEX tracks_rating   ON tracks(rating);
CREATE INDEX tracks_duration ON tracks(duration_secs);
CREATE INDEX tracks_bitrate  ON tracks(bitrate_kbps);
CREATE INDEX tracks_year     ON tracks(year);
CREATE INDEX tracks_bpm      ON tracks(bpm);
CREATE INDEX tracks_key_sort ON tracks(key_sort);
CREATE INDEX tracks_key      ON tracks(musical_key);

-- Full-text search. External-content table: the text lives once, in `tracks`.
CREATE VIRTUAL TABLE tracks_fts USING fts5(
    title, artist, album, genre, comment,
    content = 'tracks', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

INSERT INTO tracks_fts(tracks_fts) VALUES ('rebuild');

CREATE TRIGGER tracks_fts_insert AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(rowid, title, artist, album, genre, comment)
    VALUES (NEW.id, NEW.title, NEW.artist, NEW.album, NEW.genre, NEW.comment);
END;

CREATE TRIGGER tracks_fts_delete AFTER DELETE ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, genre, comment)
    VALUES ('delete', OLD.id, OLD.title, OLD.artist, OLD.album, OLD.genre, OLD.comment);
END;

CREATE TRIGGER tracks_fts_update AFTER UPDATE OF title, artist, album, genre, comment ON tracks BEGIN
    INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, genre, comment)
    VALUES ('delete', OLD.id, OLD.title, OLD.artist, OLD.album, OLD.genre, OLD.comment);
    INSERT INTO tracks_fts(rowid, title, artist, album, genre, comment)
    VALUES (NEW.id, NEW.title, NEW.artist, NEW.album, NEW.genre, NEW.comment);
END;
