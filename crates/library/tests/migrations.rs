//! Migration tests: empty to latest, v1 data to latest, backup and downgrade refusal, folder guard.
#![allow(missing_docs)]

mod common;

use opendeck_library::db::{self, DbError, SCHEMA_VERSION};
use opendeck_library::{DATABASE_FILE_NAME, Library, TrackQuery};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

const V1: &str = include_str!("../migrations/0001_init.sql");

fn v1_migrations() -> Migrations<'static> {
    Migrations::from_slice(Box::leak(Box::new([M::up(V1)])))
}

fn version(c: &Connection) -> i64 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn all_migrations_validate_and_reach_latest_from_empty() {
    db::migrations().validate().unwrap();
    let conn = db::open_in_memory().unwrap();
    assert_eq!(version(&conn), SCHEMA_VERSION);
    for table in ["tracks_fts", "playlist_tracks"] {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "{table}");
    }
}

/// A v1 database holding tracks, a playlist with ordered entries and a folder.
fn seed_v1(conn: &mut Connection) {
    v1_migrations().to_latest(conn).unwrap();
    assert_eq!(version(conn), 1);
    conn.execute_batch(
        "INSERT INTO tracks (id, path, file_size, file_mtime, title, artist, genre, date_added)
           VALUES (1, '/m/a.wav', 10, 0, 'Alpha Tune', 'Zed', 'House', 1),
                  (2, '/m/b.wav', 10, 0, 'Beta Tune', 'Yan', 'Techno', 2),
                  (3, '/m/c.wav', 10, 0, 'Gamma', 'Xi', 'House', 3);
         INSERT INTO playlists (id, kind, name, position, created_at, updated_at)
           VALUES (1, 'playlist', 'Warmup', 0, 0, 0), (2, 'folder', 'Sets', 1, 0, 0);
         INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at)
           VALUES (1, 0, 3, 5), (1, 1, 1, 6), (1, 2, 2, 7);
         INSERT INTO analysis (track_id, analyzer, analyzer_version, analyzed_at, sample_rate, bpm, musical_key)
           VALUES (2, 'old', 1, 0, 44100, 128.0, '10B');",
    )
    .unwrap();
}

#[test]
fn v1_data_survives_the_upgrade() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    seed_v1(&mut conn);
    db::migrations().to_latest(&mut conn).unwrap();
    assert_eq!(version(&conn), SCHEMA_VERSION);

    let entries: Vec<(i64, i64)> = conn
        .prepare("SELECT position, track_id FROM playlist_tracks WHERE playlist_id = 1 ORDER BY position")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(entries, vec![(0, 3), (1, 1), (2, 2)]);

    // Existing rows are searchable: the FTS index was rebuilt from `tracks`.
    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM tracks_fts WHERE tracks_fts MATCH 'tune*'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 2);

    // BPM and key were copied onto `tracks`, and stay in sync through the triggers.
    let copied = |id: i64| -> (Option<f64>, Option<String>, Option<i64>) {
        conn.query_row(
            "SELECT bpm, musical_key, key_sort FROM tracks WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
    };
    assert_eq!(copied(2), (Some(128.0), Some("10B".into()), Some(21)));
    assert_eq!(copied(1), (None, None, None));
    conn.execute(
        "UPDATE analysis SET bpm = 130.5, musical_key = '3A' WHERE track_id = 2",
        [],
    )
    .unwrap();
    assert_eq!(copied(2), (Some(130.5), Some("3A".into()), Some(6)));
    conn.execute("DELETE FROM analysis WHERE track_id = 2", [])
        .unwrap();
    assert_eq!(copied(2), (None, None, None));

    // Positions are no longer unique, entries have their own id, the same track may repeat.
    conn.execute(
        "INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at) VALUES (1, 0, 3, 9)",
        [],
    )
    .unwrap();
}

#[test]
fn upgraded_database_works_through_the_library_api() {
    let dir = common::tmp("upgrade-api");
    let path = dir.join(DATABASE_FILE_NAME);
    {
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        seed_v1(&mut conn);
    }
    let lib = Library::open(&path).unwrap();
    let page = lib
        .query_tracks(&TrackQuery {
            text: Some("tun".into()),
            ..TrackQuery::default()
        })
        .unwrap();
    assert_eq!(page.total, 2);
    let tree = lib.playlist_tree().unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(
        tree.iter()
            .find(|n| n.name == "Warmup")
            .unwrap()
            .track_count,
        3
    );
}

#[test]
fn opening_an_old_file_backs_it_up_first() {
    let dir = common::tmp("backup");
    let path = dir.join(DATABASE_FILE_NAME);
    {
        let mut conn = Connection::open(&path).unwrap();
        seed_v1(&mut conn);
    }
    drop(Library::open(&path).unwrap());
    let backup = dir.join(format!("{DATABASE_FILE_NAME}.bak-v1"));
    assert!(backup.exists(), "backup file missing");
    let old = Connection::open(&backup).unwrap();
    assert_eq!(version(&old), 1);
    let n: i64 = old
        .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);

    // An up-to-date file is not backed up again.
    let before = std::fs::metadata(&backup).unwrap().modified().unwrap();
    drop(Library::open(&path).unwrap());
    assert_eq!(
        std::fs::metadata(&backup).unwrap().modified().unwrap(),
        before
    );
}

#[test]
fn a_newer_schema_is_refused_and_left_alone() {
    let dir = common::tmp("newer");
    let path = dir.join(DATABASE_FILE_NAME);
    drop(Library::open(&path).unwrap());
    {
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 5)
            .unwrap();
    }
    match Library::open(&path) {
        Err(opendeck_library::LibraryError::Db(DbError::NewerSchema { found, supported })) => {
            assert_eq!(found, SCHEMA_VERSION + 5);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        other => panic!("expected NewerSchema, got {other:?}"),
    }
    let conn = Connection::open(&path).unwrap();
    assert_eq!(version(&conn), SCHEMA_VERSION + 5);
}

#[test]
fn folder_guard_covers_insert_update_and_kind_change() {
    let conn = db::open_in_memory().unwrap();
    conn.execute_batch(
        "INSERT INTO tracks (id, path, file_size, file_mtime, date_added) VALUES (1, '/a', 1, 0, 0);
         INSERT INTO playlists (id, kind, name, position, created_at, updated_at)
           VALUES (1, 'folder', 'F', 0, 0, 0), (2, 'playlist', 'P', 1, 0, 0), (3, 'playlist', 'Q', 2, 0, 0);
         INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at) VALUES (2, 0, 1, 0);",
    )
    .unwrap();

    let err = |sql: &str| conn.execute(sql, []).expect_err(sql).to_string();
    assert!(err("INSERT INTO playlist_tracks (playlist_id, position, track_id, added_at) VALUES (1, 0, 1, 0)")
        .contains("folder"));
    assert!(
        err("UPDATE playlist_tracks SET playlist_id = 1 WHERE playlist_id = 2").contains("folder")
    );
    assert!(err("UPDATE playlists SET kind = 'folder' WHERE id = 2").contains("tracks"));
    // An empty playlist may become a folder, and back.
    conn.execute("UPDATE playlists SET kind = 'folder' WHERE id = 3", [])
        .unwrap();
    conn.execute("UPDATE playlists SET kind = 'playlist' WHERE id = 3", [])
        .unwrap();
    // Moving entries between playlists is fine.
    conn.execute(
        "UPDATE playlist_tracks SET playlist_id = 3 WHERE playlist_id = 2",
        [],
    )
    .unwrap();
    // Only folders contain playlists.
    assert!(err("INSERT INTO playlists (parent_id, kind, name, position, created_at, updated_at) VALUES (2, 'playlist', 'x', 0, 0, 0)")
        .contains("folder"));
    assert!(err("UPDATE playlists SET parent_id = 2 WHERE id = 3").contains("folder"));
    conn.execute("UPDATE playlists SET parent_id = 1 WHERE id = 3", [])
        .unwrap();
    assert!(err("UPDATE playlists SET kind = 'playlist' WHERE id = 1").contains("children"));
}
