//! Library API tests: tracks, query, analysis, cues, playlists, sets, settings.
#![allow(missing_docs)]

mod common;

use std::time::{Duration, Instant};

use opendeck_contracts::analysis::{AnalysisResult, Loudness};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::{Cue, CueColor, CueKind, HotCueIndex};
use opendeck_contracts::ids::{PlaylistId, TrackId};
use opendeck_contracts::key::MusicalKey;
use opendeck_contracts::peaks::{PeakLevel, PeakPoint};
use opendeck_contracts::units::{Bpm, Seconds};
use opendeck_library::{
    AnalysisStatus, AnalyzerStamp, Library, NewTrack, PlaylistKind, SavedSetEntry, SortColumn,
    TrackMeta, TrackQuery, TrackTech,
};

fn track(path: &str, title: &str, artist: &str) -> NewTrack {
    NewTrack {
        path: path.into(),
        file_size: 100,
        meta: TrackMeta {
            title: Some(title.into()),
            artist: Some(artist.into()),
            ..TrackMeta::default()
        },
        ..NewTrack::default()
    }
}

fn stamp(version: u32) -> AnalyzerStamp {
    AnalyzerStamp {
        analyzer: "fake".into(),
        version,
        sample_rate: 44_100,
    }
}

fn result(bpm: f64, key: &str) -> AnalysisResult {
    AnalysisResult {
        bpm: Some(Bpm(bpm)),
        beat_grid: BeatGrid::new(Seconds(0.25), Bpm(bpm), 4).ok(),
        key: MusicalKey::from_camelot(key).ok(),
        loudness: Some(Loudness {
            integrated_lufs: -9.5,
            true_peak_dbtp: -0.5,
        }),
        peaks: vec![
            PeakLevel {
                sample_rate: 44_100,
                frames_per_point: 128,
                points: vec![
                    PeakPoint {
                        peak: 1,
                        low: 2,
                        mid: 3,
                        high: 4,
                    };
                    3
                ],
            },
            PeakLevel {
                sample_rate: 44_100,
                frames_per_point: 1024,
                points: vec![
                    PeakPoint {
                        peak: 9,
                        low: 8,
                        mid: 7,
                        high: 6,
                    };
                    1
                ],
            },
        ],
    }
}

// ---- tracks ----

#[test]
fn track_crud_and_cascades() {
    let lib = Library::open_in_memory().unwrap();
    let mut t = track("/m/a.wav", "Alpha", "Zed");
    t.tech = TrackTech {
        duration_secs: Some(200.5),
        sample_rate: Some(44_100),
        channels: Some(2),
        bitrate_kbps: Some(320),
        codec: Some("mp3".into()),
    };
    t.meta.year = Some(1999);
    let id = lib.insert_track(&t).unwrap();
    assert!(lib.insert_track(&t).is_err(), "path is unique");

    let got = lib.get_track(id).unwrap().unwrap();
    assert_eq!(got.meta.title.as_deref(), Some("Alpha"));
    assert_eq!(got.tech.bitrate_kbps, Some(320));
    assert_eq!(got.analysis_status, AnalysisStatus::Pending);
    assert_eq!(lib.track_by_path("/m/a.wav").unwrap(), Some(id));

    let mut meta = got.meta.clone();
    meta.title = Some("Alpha Two".into());
    meta.genre = Some("House".into());
    lib.update_track_meta(id, &meta).unwrap();
    lib.set_rating(id, 4).unwrap();
    assert!(lib.set_rating(id, 6).is_err());
    lib.set_color(id, Some(0x112233)).unwrap();
    lib.mark_played(id).unwrap();
    let got = lib.get_track(id).unwrap().unwrap();
    assert_eq!(got.meta.title.as_deref(), Some("Alpha Two"));
    assert_eq!(
        (got.rating, got.color, got.play_count),
        (4, Some(0x112233), 1)
    );
    assert!(got.last_played_at.is_some());

    // Artwork is deduped by hash.
    let a1 = lib.store_artwork("image/png", b"png-bytes").unwrap();
    let a2 = lib.store_artwork("image/png", b"png-bytes").unwrap();
    let a3 = lib.store_artwork("image/png", b"other").unwrap();
    assert_eq!(a1, a2);
    assert_ne!(a1, a3);
    assert_eq!(lib.artwork(a1).unwrap().unwrap().1, b"png-bytes");

    // Everything hanging off the track cascades; saved sets keep their snapshot.
    lib.save_analysis(id, &stamp(1), &result(120.0, "8A"))
        .unwrap();
    lib.add_cue(
        id,
        &Cue {
            id: None,
            kind: CueKind::Memory,
            start: Seconds(1.0),
            end: None,
            color: None,
            name: String::new(),
        },
    )
    .unwrap();
    let pl = lib.create_playlist(None, "P").unwrap();
    lib.add_tracks_to_playlist(pl, &[id], None).unwrap();
    let set = lib
        .create_saved_set(
            "Friday",
            "",
            None,
            None,
            &[SavedSetEntry {
                track: Some(id),
                ..SavedSetEntry::default()
            }],
        )
        .unwrap();
    assert_eq!(lib.delete_tracks(&[id]).unwrap(), 1);
    assert!(lib.get_track(id).unwrap().is_none());
    assert!(lib.get_analysis(id).unwrap().is_none());
    assert!(lib.cues(id).unwrap().is_empty());
    assert!(lib.playlist_track_ids(pl).unwrap().is_empty());
    let entries = lib.saved_set_entries(set).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].track, None);
    assert_eq!(
        entries[0].title.as_deref(),
        Some("Alpha Two"),
        "title snapshot survives"
    );
}

#[test]
fn relink_updates_path_and_clears_missing() {
    let lib = Library::open_in_memory().unwrap();
    let id = lib.insert_track(&track("/old/a.wav", "A", "B")).unwrap();
    lib.set_missing(id, true).unwrap();
    assert!(lib.get_track(id).unwrap().unwrap().missing);
    lib.relink_track(id, "/new/a.wav").unwrap();
    let t = lib.get_track(id).unwrap().unwrap();
    assert_eq!((t.path.as_str(), t.missing), ("/new/a.wav", false));
}

// ---- query ----

const GENRES: [&str; 4] = ["House", "Techno", "Drum and Bass", "Ambient"];
const KEYS: [&str; 6] = ["1A", "8A", "10B", "12A", "2B", "5A"];

/// `n` synthetic tracks with deterministic, varied fields; every third one is unanalysed.
fn populate(lib: &Library, n: usize) -> Vec<TrackId> {
    let mut x = 0x9E37_79B9_u32;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    };
    let rows: Vec<NewTrack> = (0..n)
        .map(|i| {
            let r = next();
            NewTrack {
                path: format!("/music/{i:06}.flac"),
                file_size: 1000 + i as u64,
                meta: TrackMeta {
                    title: (i % 17 != 0).then(|| {
                        format!(
                            "Title {} {}",
                            r % 977,
                            if i % 2 == 0 { "Alpha" } else { "beta" }
                        )
                    }),
                    artist: Some(format!("Artist {}", r % 331)),
                    album: Some(format!("Album {}", r % 101)),
                    genre: (i % 11 != 0).then(|| GENRES[(r % 4) as usize].to_string()),
                    year: Some(1980 + (r % 45) as i32),
                    comment: (i % 5 == 0).then(|| "needle in the haystack".to_string()),
                    ..TrackMeta::default()
                },
                tech: TrackTech {
                    duration_secs: Some(f64::from(60 + r % 600)),
                    bitrate_kbps: Some(128 + (r % 5) * 64),
                    ..TrackTech::default()
                },
                rating: (r % 6) as u8,
                date_added: Some(1_700_000_000_000 + i64::from(r % 1_000_000)),
                ..NewTrack::default()
            }
        })
        .collect();
    let ids = lib.insert_tracks(&rows).unwrap();
    for (i, id) in ids.iter().enumerate() {
        if i % 3 != 0 {
            let r = (i as u32).wrapping_mul(2_654_435_761);
            let mut res = result(
                70.0 + f64::from(r % 1300) / 10.0,
                KEYS[(r >> 8) as usize % KEYS.len()],
            );
            res.peaks.clear();
            lib.save_analysis(*id, &stamp(1), &res).unwrap();
        }
    }
    ids
}

fn all_rows(lib: &Library, mut q: TrackQuery) -> Vec<opendeck_library::TrackRow> {
    q.offset = 0;
    q.limit = u32::MAX;
    lib.query_tracks(&q).unwrap().rows
}

#[test]
fn every_sort_column_orders_correctly_with_nulls_last() {
    let lib = Library::open_in_memory().unwrap();
    populate(&lib, 600);
    let camelot = |k: &str| {
        (
            k[..k.len() - 1].parse::<u32>().unwrap(),
            k.as_bytes()[k.len() - 1],
        )
    };
    for desc in [false, true] {
        for sort in [
            SortColumn::Title,
            SortColumn::Artist,
            SortColumn::Album,
            SortColumn::Genre,
            SortColumn::Duration,
            SortColumn::Bitrate,
            SortColumn::Year,
            SortColumn::DateAdded,
            SortColumn::Bpm,
            SortColumn::Key,
            SortColumn::Rating,
            SortColumn::AnalysisStatus,
        ] {
            let rows = all_rows(
                &lib,
                TrackQuery {
                    sort,
                    descending: desc,
                    ..TrackQuery::default()
                },
            );
            assert_eq!(rows.len(), 600);
            // Comparable key per row: None sorts after every Some.
            let key = |r: &opendeck_library::TrackRow| -> Option<String> {
                match sort {
                    SortColumn::Title => r.title.as_ref().map(|s| s.to_lowercase()),
                    SortColumn::Artist => r.artist.as_ref().map(|s| s.to_lowercase()),
                    SortColumn::Album => r.album.as_ref().map(|s| s.to_lowercase()),
                    SortColumn::Genre => r.genre.as_ref().map(|s| s.to_lowercase()),
                    SortColumn::Duration => r.duration_secs.map(|v| format!("{v:012.3}")),
                    SortColumn::Bitrate => r.bitrate_kbps.map(|v| format!("{v:012}")),
                    SortColumn::Year => r.year.map(|v| format!("{v:012}")),
                    SortColumn::DateAdded => Some(format!("{:020}", r.date_added)),
                    SortColumn::Bpm => r.bpm.map(|v| format!("{v:012.3}")),
                    SortColumn::Key => r.key.as_ref().map(|k| {
                        let (n, l) = camelot(k);
                        format!("{n:03}{}", l as char)
                    }),
                    SortColumn::Rating => Some(format!("{:03}", r.rating)),
                    SortColumn::AnalysisStatus => Some(r.analysis_status.as_str().to_string()),
                }
            };
            let keys: Vec<Option<String>> = rows.iter().map(key).collect();
            let first_none = keys.iter().position(Option::is_none).unwrap_or(keys.len());
            assert!(
                keys[first_none..].iter().all(Option::is_none),
                "{sort:?} desc={desc}: NULLs are not all last"
            );
            let present: Vec<&String> = keys[..first_none]
                .iter()
                .map(|k| k.as_ref().unwrap())
                .collect();
            let ok = present
                .windows(2)
                .all(|w| if desc { w[0] >= w[1] } else { w[0] <= w[1] });
            assert!(ok, "{sort:?} desc={desc}: not ordered");
        }
    }
}

#[test]
fn filters_match_a_reference_scan() {
    let lib = Library::open_in_memory().unwrap();
    populate(&lib, 900);
    let base = all_rows(&lib, TrackQuery::default());
    let count = |q: TrackQuery| lib.query_tracks(&q).unwrap().total as usize;

    // BPM range (unanalysed rows have no BPM and never match).
    let expect = base
        .iter()
        .filter(|r| r.bpm.is_some_and(|b| (100.0..=128.0).contains(&b)))
        .count();
    assert!(expect > 0);
    assert_eq!(
        count(TrackQuery {
            bpm_min: Some(100.0),
            bpm_max: Some(128.0),
            ..TrackQuery::default()
        }),
        expect
    );
    // Open-ended range.
    let expect = base
        .iter()
        .filter(|r| r.bpm.is_some_and(|b| b >= 150.0))
        .count();
    assert_eq!(
        count(TrackQuery {
            bpm_min: Some(150.0),
            ..TrackQuery::default()
        }),
        expect
    );

    // Keys.
    let expect = base
        .iter()
        .filter(|r| matches!(r.key.as_deref(), Some("8A" | "10B")))
        .count();
    assert!(expect > 0);
    assert_eq!(
        count(TrackQuery {
            keys: vec!["8A".into(), "10B".into()],
            ..TrackQuery::default()
        }),
        expect
    );

    // Genre is case-insensitive and exact.
    let expect = base
        .iter()
        .filter(|r| r.genre.as_deref() == Some("Techno"))
        .count();
    assert_eq!(
        count(TrackQuery {
            genre: Some("tECHno".into()),
            ..TrackQuery::default()
        }),
        expect
    );

    // Rating and status.
    let expect = base.iter().filter(|r| r.rating >= 4).count();
    assert_eq!(
        count(TrackQuery {
            min_rating: Some(4),
            ..TrackQuery::default()
        }),
        expect
    );
    let expect = base
        .iter()
        .filter(|r| r.analysis_status == AnalysisStatus::Pending)
        .count();
    assert_eq!(expect, 300);
    assert_eq!(
        count(TrackQuery {
            status: Some(AnalysisStatus::Pending),
            ..TrackQuery::default()
        }),
        expect
    );

    // Combined, with paging: total ignores the page.
    let q = TrackQuery {
        bpm_min: Some(90.0),
        min_rating: Some(2),
        genre: Some("House".into()),
        sort: SortColumn::Bpm,
        limit: 7,
        offset: 3,
        ..TrackQuery::default()
    };
    let full = all_rows(&lib, q.clone());
    let page = lib.query_tracks(&q).unwrap();
    assert_eq!(page.total as usize, full.len());
    assert_eq!(page.rows, full[3..(3 + 7).min(full.len())]);
}

#[test]
fn text_search_covers_title_artist_album_genre_comment() {
    let lib = Library::open_in_memory().unwrap();
    let mk = |path: &str, f: &dyn Fn(&mut TrackMeta)| {
        let mut t = track(path, "Untitled", "Nobody");
        f(&mut t.meta);
        lib.insert_track(&t).unwrap()
    };
    let by_title = mk("/1", &|m| m.title = Some("Blue Monday".into()));
    let by_artist = mk("/2", &|m| m.artist = Some("The Bluebells".into()));
    let by_album = mk("/3", &|m| m.album = Some("Navy Blues".into()));
    let by_genre = mk("/4", &|m| m.genre = Some("Blue-eyed soul".into()));
    let by_comment = mk("/5", &|m| m.comment = Some("so blue".into()));
    let diacritic = mk("/6", &|m| m.title = Some("Café del Mar".into()));
    mk("/7", &|_| {});

    let ids = |text: &str| -> Vec<TrackId> {
        let mut v: Vec<_> = all_rows(
            &lib,
            TrackQuery {
                text: Some(text.into()),
                ..TrackQuery::default()
            },
        )
        .into_iter()
        .map(|r| r.id)
        .collect();
        v.sort();
        v
    };
    assert_eq!(
        ids("blue"),
        vec![by_title, by_artist, by_album, by_genre, by_comment]
    );
    assert_eq!(
        ids("BLU"),
        vec![by_title, by_artist, by_album, by_genre, by_comment],
        "prefix, any case"
    );
    let comment_row = all_rows(&lib, TrackQuery::default())
        .into_iter()
        .find(|r| r.id == by_comment)
        .unwrap();
    assert_eq!(
        comment_row.comment.as_deref(),
        Some("so blue"),
        "rows carry the comment"
    );
    assert_eq!(ids("blue monday"), vec![by_title], "every word must match");
    assert_eq!(ids("cafe"), vec![diacritic], "diacritics fold");
    assert!(ids("zzzz").is_empty());
    // Punctuation and FTS operators are data, not syntax.
    assert_eq!(
        ids("\"blue\"* -("),
        vec![by_title, by_artist, by_album, by_genre, by_comment]
    );
    assert_eq!(
        all_rows(
            &lib,
            TrackQuery {
                text: Some("  ".into()),
                ..TrackQuery::default()
            }
        )
        .len(),
        7
    );

    // The index follows updates and deletes.
    let mut meta = lib.get_track(by_title).unwrap().unwrap().meta;
    meta.title = Some("Red Monday".into());
    lib.update_track_meta(by_title, &meta).unwrap();
    assert!(!ids("blue").contains(&by_title));
    assert_eq!(ids("red"), vec![by_title]);
    lib.delete_tracks(&[by_title]).unwrap();
    assert!(ids("red").is_empty());
}

#[test]
fn query_over_50k_tracks_stays_under_50_ms() {
    let lib = Library::open_in_memory().unwrap();
    let t0 = Instant::now();
    populate(&lib, 50_000);
    eprintln!("populated 50k tracks in {:?}", t0.elapsed());
    // Reader connections in the pool see a file; in-memory shares the writer, so this also
    // measures the single-connection case.
    let cases: Vec<(&str, TrackQuery)> = vec![
        ("default (date added)", TrackQuery::default()),
        (
            "title",
            TrackQuery {
                sort: SortColumn::Title,
                ..TrackQuery::default()
            },
        ),
        (
            "artist desc",
            TrackQuery {
                sort: SortColumn::Artist,
                descending: true,
                ..TrackQuery::default()
            },
        ),
        (
            "bpm",
            TrackQuery {
                sort: SortColumn::Bpm,
                ..TrackQuery::default()
            },
        ),
        (
            "key",
            TrackQuery {
                sort: SortColumn::Key,
                ..TrackQuery::default()
            },
        ),
        (
            "duration",
            TrackQuery {
                sort: SortColumn::Duration,
                ..TrackQuery::default()
            },
        ),
        (
            "rating",
            TrackQuery {
                sort: SortColumn::Rating,
                ..TrackQuery::default()
            },
        ),
        (
            "status",
            TrackQuery {
                sort: SortColumn::AnalysisStatus,
                ..TrackQuery::default()
            },
        ),
        (
            "deep page",
            TrackQuery {
                sort: SortColumn::Title,
                offset: 49_900,
                ..TrackQuery::default()
            },
        ),
        (
            "text 'needle'",
            TrackQuery {
                text: Some("needle".into()),
                sort: SortColumn::Title,
                ..TrackQuery::default()
            },
        ),
        (
            "text prefix 'alp'",
            TrackQuery {
                text: Some("alp".into()),
                sort: SortColumn::Bpm,
                ..TrackQuery::default()
            },
        ),
        (
            "bpm range + genre + rating",
            TrackQuery {
                bpm_min: Some(120.0),
                bpm_max: Some(130.0),
                genre: Some("House".into()),
                min_rating: Some(3),
                sort: SortColumn::Title,
                ..TrackQuery::default()
            },
        ),
        (
            "keys",
            TrackQuery {
                keys: vec!["8A".into(), "9A".into()],
                sort: SortColumn::Bpm,
                ..TrackQuery::default()
            },
        ),
    ];
    let budget = Duration::from_millis(50);
    let mut worst = Duration::ZERO;
    for (name, q) in cases {
        // Best of three: the first run compiles the statement and warms the page cache.
        let mut best = Duration::MAX;
        let mut total = 0;
        for _ in 0..3 {
            let t = Instant::now();
            let page = lib.query_tracks(&q).unwrap();
            best = best.min(t.elapsed());
            total = page.total;
        }
        eprintln!("{name:<28} {best:>10.2?}  ({total} matches)");
        worst = worst.max(best);
        assert!(best < budget, "{name} took {best:?}");
    }
    eprintln!("worst case {worst:?}");
}

// ---- analysis ----

#[test]
fn analysis_round_trip_and_peak_blobs() {
    let lib = Library::open_in_memory().unwrap();
    let id = lib.insert_track(&track("/a", "A", "B")).unwrap();
    assert!(lib.get_analysis(id).unwrap().is_none());
    lib.save_analysis(id, &stamp(3), &result(126.0, "8A"))
        .unwrap();

    let a = lib.get_analysis(id).unwrap().unwrap();
    assert_eq!(
        (a.analyzer.as_str(), a.analyzer_version, a.sample_rate),
        ("fake", 3, 44_100)
    );
    assert_eq!(a.bpm, Some(126.0));
    assert_eq!(a.key.unwrap().camelot(), "8A");
    let g = a.beat_grid.unwrap();
    assert_eq!(
        (g.anchor(), g.bpm(), g.beats_per_bar()),
        (Seconds(0.25), Bpm(126.0), 4)
    );
    assert_eq!(a.loudness.unwrap().integrated_lufs, -9.5);
    assert!(!a.grid_locked);
    assert_eq!(lib.analysis_status(id).unwrap(), Some(AnalysisStatus::Done));

    let levels = lib.peak_levels(id).unwrap();
    assert_eq!(levels.len(), 2);
    assert_eq!((levels[0].level, levels[0].frames_per_point), (0, 128));
    assert_eq!((levels[1].level, levels[1].frames_per_point), (1, 1024));
    let blob = lib.get_peaks(id, 0).unwrap().unwrap();
    assert_eq!(blob.len() as u32, levels[0].byte_len);
    let decoded = PeakLevel::decode(&blob).unwrap();
    assert_eq!(
        decoded,
        result(126.0, "8A").peaks[0],
        "stored in the wire layout, unchanged"
    );
    assert!(lib.get_peaks(id, 7).unwrap().is_none());

    // The collection view sees BPM and key.
    let row = &all_rows(&lib, TrackQuery::default())[0];
    assert_eq!((row.bpm, row.key.as_deref()), (Some(126.0), Some("8A")));
}

#[test]
fn locked_grid_survives_reanalysis_but_other_results_update() {
    let lib = Library::open_in_memory().unwrap();
    let id = lib.insert_track(&track("/a", "A", "B")).unwrap();
    lib.save_analysis(id, &stamp(1), &result(120.0, "8A"))
        .unwrap();

    let mine = BeatGrid::new(Seconds(0.5), Bpm(121.5), 4).unwrap();
    lib.set_beat_grid(id, &mine, true).unwrap();
    assert!(lib.get_analysis(id).unwrap().unwrap().grid_locked);

    let out = lib
        .save_analysis(id, &stamp(2), &result(140.0, "3B"))
        .unwrap();
    assert!(out.grid_kept);
    let a = lib.get_analysis(id).unwrap().unwrap();
    assert_eq!(a.beat_grid, Some(mine), "grid kept");
    assert_eq!(a.bpm, Some(121.5), "bpm follows the kept grid");
    assert_eq!(a.key.unwrap().camelot(), "3B", "key updated");
    assert_eq!(a.analyzer_version, 2);

    // Unlocked: the new grid wins.
    lib.set_grid_locked(id, false).unwrap();
    let out = lib
        .save_analysis(id, &stamp(3), &result(140.0, "3B"))
        .unwrap();
    assert!(!out.grid_kept);
    let a = lib.get_analysis(id).unwrap().unwrap();
    assert_eq!(a.beat_grid.unwrap().bpm(), Bpm(140.0));
    assert_eq!(a.bpm, Some(140.0));

    // A partial result keeps earlier values it does not provide.
    let partial = AnalysisResult {
        key: MusicalKey::from_camelot("1A").ok(),
        ..AnalysisResult::default()
    };
    lib.save_analysis(id, &stamp(4), &partial).unwrap();
    let a = lib.get_analysis(id).unwrap().unwrap();
    assert_eq!(a.key.unwrap().camelot(), "1A");
    assert_eq!(a.bpm, Some(140.0));
    assert!(a.beat_grid.is_some() && a.loudness.is_some());
    assert_eq!(lib.peak_levels(id).unwrap().len(), 2, "peaks kept");
}

#[test]
fn stale_detection_by_analyzer_version() {
    let lib = Library::open_in_memory().unwrap();
    let old = lib.insert_track(&track("/old", "O", "x")).unwrap();
    let new = lib.insert_track(&track("/new", "N", "x")).unwrap();
    let never = lib.insert_track(&track("/never", "V", "x")).unwrap();
    lib.save_analysis(old, &stamp(1), &result(120.0, "8A"))
        .unwrap();
    lib.save_analysis(new, &stamp(2), &result(120.0, "8A"))
        .unwrap();
    assert_eq!(
        lib.tracks_needing_analysis("fake", 2).unwrap(),
        vec![old, never]
    );
    assert_eq!(lib.mark_stale("fake", 2).unwrap(), 1);
    assert_eq!(
        lib.analysis_status(old).unwrap(),
        Some(AnalysisStatus::Stale)
    );
    assert_eq!(
        lib.analysis_status(new).unwrap(),
        Some(AnalysisStatus::Done)
    );
}

// ---- cues ----

fn cue(kind: CueKind, start: f64, end: Option<f64>, name: &str) -> Cue {
    Cue {
        id: None,
        kind,
        start: Seconds(start),
        end: end.map(Seconds),
        color: Some(CueColor { r: 1, g: 2, b: 3 }),
        name: name.into(),
    }
}

#[test]
fn cues_crud() {
    let lib = Library::open_in_memory().unwrap();
    let t = lib.insert_track(&track("/a", "A", "B")).unwrap();
    let other = lib.insert_track(&track("/b", "B", "B")).unwrap();
    let slot = |i| CueKind::Hot {
        index: HotCueIndex::new(i).unwrap(),
    };

    let h0 = lib.add_cue(t, &cue(slot(0), 4.0, None, "drop")).unwrap();
    let m = lib
        .add_cue(t, &cue(CueKind::Memory, 1.0, None, "intro"))
        .unwrap();
    let l = lib
        .add_cue(t, &cue(CueKind::Loop, 8.0, Some(16.0), "loop"))
        .unwrap();
    lib.add_cue(other, &cue(slot(0), 2.0, None, "other track"))
        .unwrap();

    let cues = lib.cues(t).unwrap();
    assert_eq!(
        cues.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["intro", "drop", "loop"],
        "by position"
    );
    let got = lib.get_cue(l).unwrap().unwrap();
    assert_eq!(
        (got.kind, got.start, got.end),
        (CueKind::Loop, Seconds(8.0), Some(Seconds(16.0)))
    );
    assert_eq!(got.color, Some(CueColor { r: 1, g: 2, b: 3 }));
    assert_eq!(got.id, Some(l));

    // Setting a used pad replaces its cue.
    let h0b = lib.add_cue(t, &cue(slot(0), 5.0, None, "drop 2")).unwrap();
    assert_ne!(h0, h0b);
    assert!(lib.get_cue(h0).unwrap().is_none());
    assert_eq!(lib.cues(t).unwrap().len(), 3);

    // Update: move, rename, change slot (frees and takes slots).
    let mut c = lib.get_cue(h0b).unwrap().unwrap();
    c.start = Seconds(6.0);
    c.name = "moved".into();
    c.kind = slot(3);
    lib.update_cue(&c).unwrap();
    let c = lib.get_cue(h0b).unwrap().unwrap();
    assert_eq!(
        (c.start, c.name.as_str(), c.kind),
        (Seconds(6.0), "moved", slot(3))
    );
    let mut m_cue = lib.get_cue(m).unwrap().unwrap();
    m_cue.kind = slot(3); // takes a used slot: the old holder goes
    lib.update_cue(&m_cue).unwrap();
    assert!(lib.get_cue(h0b).unwrap().is_none());

    // Validation.
    assert!(
        lib.add_cue(t, &cue(CueKind::Loop, 1.0, None, "bad"))
            .is_err()
    );
    assert!(
        lib.add_cue(t, &cue(CueKind::Memory, -1.0, None, "bad"))
            .is_err()
    );
    assert!(
        lib.add_cue(t, &cue(CueKind::Memory, 2.0, Some(1.0), "bad"))
            .is_err()
    );
    let mut no_id = cue(CueKind::Memory, 1.0, None, "x");
    assert!(lib.update_cue(&no_id).is_err());
    no_id.id = Some(opendeck_contracts::ids::CueId(9999));
    assert!(lib.update_cue(&no_id).is_err());

    assert!(lib.delete_cue(l).unwrap());
    assert!(!lib.delete_cue(l).unwrap());
    assert_eq!(lib.clear_cues(t).unwrap(), 1);
    assert_eq!(lib.cues(other).unwrap().len(), 1);
}

// ---- playlists ----

fn make_tracks(lib: &Library, n: usize) -> Vec<TrackId> {
    let rows: Vec<NewTrack> = (0..n)
        .map(|i| track(&format!("/t/{i}"), &format!("T{i}"), "A"))
        .collect();
    lib.insert_tracks(&rows).unwrap()
}

fn order(lib: &Library, pl: PlaylistId) -> Vec<i64> {
    let e = lib.playlist_entries(pl).unwrap();
    for (i, entry) in e.iter().enumerate() {
        assert_eq!(entry.position as usize, i, "positions stay dense");
    }
    e.iter().map(|e| e.entry_id).collect()
}

#[test]
fn playlist_reorder_stress_never_hits_a_position_conflict() {
    let lib = Library::open_in_memory().unwrap();
    let tracks = make_tracks(&lib, 400);
    let pl = lib.create_playlist(None, "Stress").unwrap();
    let mut model: Vec<i64> = lib
        .add_tracks_to_playlist(pl, &tracks[..300], None)
        .unwrap();

    let mut x = 0x1234_5678_u32;
    let mut rnd = move |n: usize| {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x as usize) % n.max(1)
    };
    let mut spare = tracks[300..].to_vec();
    for step in 0..500 {
        match rnd(4) {
            0 if !spare.is_empty() => {
                // Insert a block at a random index (shifts everything after it).
                let n = 1 + rnd(3).min(spare.len() - 1);
                let block: Vec<TrackId> = spare.drain(..n).collect();
                let at = rnd(model.len() + 1);
                let ids = lib.add_tracks_to_playlist(pl, &block, Some(at)).unwrap();
                model.splice(at..at, ids);
            }
            1 if model.len() > 10 => {
                // Remove a few.
                let k = 1 + rnd(3);
                let mut victims: Vec<i64> = (0..k).map(|_| model[rnd(model.len())]).collect();
                victims.sort_unstable();
                victims.dedup();
                lib.remove_playlist_entries(pl, &victims).unwrap();
                model.retain(|e| !victims.contains(e));
            }
            _ => {
                // Move a random block to a random index.
                let k = 1 + rnd(5);
                let mut picked: Vec<i64> = (0..k).map(|_| model[rnd(model.len())]).collect();
                picked.dedup();
                picked.sort_unstable();
                picked.dedup();
                let to = rnd(model.len() + 1);
                lib.move_playlist_entries(pl, &picked, to).unwrap();
                let moving: Vec<i64> = model
                    .iter()
                    .copied()
                    .filter(|e| picked.contains(e))
                    .collect();
                let mut rest: Vec<i64> = model
                    .iter()
                    .copied()
                    .filter(|e| !picked.contains(e))
                    .collect();
                let at = to.min(rest.len());
                rest.splice(at..at, moving);
                model = rest;
            }
        }
        if step % 25 == 0 {
            assert_eq!(order(&lib, pl), model, "step {step}");
        }
    }
    assert_eq!(order(&lib, pl), model);
}

#[test]
fn bulk_insert_at_top_of_a_large_playlist() {
    let lib = Library::open_in_memory().unwrap();
    let tracks = make_tracks(&lib, 6000);
    let pl = lib.create_playlist(None, "Big").unwrap();
    lib.add_tracks_to_playlist(pl, &tracks[..5000], None)
        .unwrap();
    let t = Instant::now();
    let ids = lib
        .add_tracks_to_playlist(pl, &tracks[5000..], Some(0))
        .unwrap();
    eprintln!("insert 1000 at the top of 5000: {:?}", t.elapsed());
    assert_eq!(ids.len(), 1000);
    let o = order(&lib, pl);
    assert_eq!(o.len(), 6000);
    assert_eq!(&o[..1000], &ids[..]);
    // The same track twice is allowed.
    lib.add_tracks_to_playlist(pl, &[tracks[0], tracks[0]], Some(1))
        .unwrap();
    assert_eq!(
        lib.playlist_track_ids(pl)
            .unwrap()
            .iter()
            .filter(|t| **t == tracks[0])
            .count(),
        3
    );
}

#[test]
fn playlist_tree_folders_and_moves() {
    let lib = Library::open_in_memory().unwrap();
    let tracks = make_tracks(&lib, 3);
    let sets = lib.create_folder(None, "Sets").unwrap();
    let gigs = lib.create_folder(Some(sets), "Gigs").unwrap();
    let warm = lib.create_playlist(Some(gigs), "Warmup").unwrap();
    let top = lib.create_playlist(None, "Top").unwrap();
    lib.add_tracks_to_playlist(warm, &tracks, None).unwrap();

    // Folders hold no tracks; playlists hold no children.
    assert!(lib.add_tracks_to_playlist(sets, &tracks, None).is_err());
    assert!(lib.create_playlist(Some(top), "child").is_err());
    assert!(lib.create_folder(Some(top), "child").is_err());

    let tree = lib.playlist_tree().unwrap();
    let node = |name: &str| tree.iter().find(|n| n.name == name).unwrap().clone();
    assert_eq!(node("Warmup").track_count, 3);
    assert_eq!(node("Sets").kind, PlaylistKind::Folder);
    assert_eq!(node("Gigs").parent, Some(sets));
    assert_eq!((node("Sets").position, node("Top").position), (0, 1));

    // Cycles are refused.
    assert!(lib.move_playlist(sets, Some(gigs), 0).is_err());
    assert!(lib.move_playlist(sets, Some(sets), 0).is_err());
    assert!(
        lib.move_playlist(warm, Some(top), 0).is_err(),
        "target is a playlist"
    );

    // Move and reorder siblings; gaps close on both sides.
    lib.move_playlist(top, Some(sets), 0).unwrap();
    let tree = lib.playlist_tree().unwrap();
    let kids: Vec<_> = tree
        .iter()
        .filter(|n| n.parent == Some(sets))
        .map(|n| (n.name.as_str(), n.position))
        .collect();
    assert_eq!(kids, vec![("Top", 0), ("Gigs", 1)]);
    assert!(
        tree.iter()
            .filter(|n| n.parent.is_none())
            .all(|n| n.position == 0)
    );
    lib.move_playlist(gigs, None, 0).unwrap();
    lib.rename_playlist(warm, "Warm up").unwrap();

    // Deleting a folder takes its subtree and entries, never the tracks.
    lib.delete_playlist(gigs).unwrap();
    assert!(
        lib.playlist_tree()
            .unwrap()
            .iter()
            .all(|n| n.name != "Warm up")
    );
    assert_eq!(lib.track_count().unwrap(), 3);
    assert!(lib.delete_playlist(gigs).is_err());
}

#[test]
fn removing_entries_closes_gaps() {
    let lib = Library::open_in_memory().unwrap();
    let tracks = make_tracks(&lib, 6);
    let pl = lib.create_playlist(None, "P").unwrap();
    let ids = lib.add_tracks_to_playlist(pl, &tracks, None).unwrap();
    lib.remove_playlist_entries(pl, &[ids[1], ids[4]]).unwrap();
    assert_eq!(order(&lib, pl), vec![ids[0], ids[2], ids[3], ids[5]]);
    // Entries of another playlist are not touched.
    let other = lib.create_playlist(None, "Q").unwrap();
    lib.add_tracks_to_playlist(other, &tracks[..1], None)
        .unwrap();
    lib.remove_playlist_entries(other, &[ids[0]]).unwrap();
    assert_eq!(order(&lib, pl).len(), 4);
    assert!(
        lib.move_playlist_entries(other, &[ids[0]], 0).is_err(),
        "entry not in that playlist"
    );
}

// ---- saved sets, settings ----

#[test]
fn saved_sets_keep_order_and_snapshots() {
    let lib = Library::open_in_memory().unwrap();
    let tracks = make_tracks(&lib, 3);
    let set = lib
        .create_saved_set(
            "Friday",
            "club night",
            Some("{\"decks\":2}"),
            Some(1000),
            &[
                SavedSetEntry {
                    track: Some(tracks[2]),
                    deck: Some(0),
                    played_at: Some(1100),
                    ..SavedSetEntry::default()
                },
                SavedSetEntry {
                    track: Some(tracks[0]),
                    deck: Some(1),
                    played_at: Some(1200),
                    ..SavedSetEntry::default()
                },
            ],
        )
        .unwrap();
    lib.append_to_saved_set(
        set,
        &SavedSetEntry {
            track: Some(tracks[1]),
            deck: Some(0),
            ..SavedSetEntry::default()
        },
    )
    .unwrap();
    assert!(
        lib.append_to_saved_set(
            set,
            &SavedSetEntry {
                deck: Some(9),
                ..SavedSetEntry::default()
            }
        )
        .is_err()
    );
    assert!(
        lib.append_to_saved_set(999, &SavedSetEntry::default())
            .is_err()
    );

    let e = lib.saved_set_entries(set).unwrap();
    assert_eq!(
        e.iter().map(|e| e.track).collect::<Vec<_>>(),
        vec![Some(tracks[2]), Some(tracks[0]), Some(tracks[1])]
    );
    assert_eq!(e[0].title.as_deref(), Some("T2"));
    let sets = lib.saved_sets().unwrap();
    assert_eq!((sets[0].name.as_str(), sets[0].track_count), ("Friday", 3));
    assert_eq!(sets[0].session_state.as_deref(), Some("{\"decks\":2}"));
    lib.update_saved_set(set, "Friday late", "", None).unwrap();
    assert_eq!(lib.saved_sets().unwrap()[0].name, "Friday late");
    assert!(lib.delete_saved_set(set).unwrap());
    assert!(lib.saved_set_entries(set).unwrap().is_empty());
}

#[test]
fn settings_round_trip() {
    let lib = Library::open_in_memory().unwrap();
    assert_eq!(lib.get_setting("audio").unwrap(), None);
    lib.set_setting("audio", "{\"rate\":48000}").unwrap();
    lib.set_setting("audio", "{\"rate\":44100}").unwrap();
    assert_eq!(
        lib.get_setting("audio").unwrap().as_deref(),
        Some("{\"rate\":44100}")
    );
    let v: serde_json::Value = lib.get_setting_json("audio").unwrap().unwrap();
    assert_eq!(v["rate"], 44100);
    lib.set_setting_json("theme", &vec!["dark", "compact"])
        .unwrap();
    let t: Vec<String> = lib.get_setting_json("theme").unwrap().unwrap();
    assert_eq!(t, ["dark", "compact"]);
    assert!(lib.delete_setting("audio").unwrap());
    assert!(!lib.delete_setting("audio").unwrap());
}

#[test]
fn file_library_reads_on_separate_connections_while_writing() {
    let dir = common::tmp("file-lib");
    let lib = Library::open(&dir.join(opendeck_library::DATABASE_FILE_NAME)).unwrap();
    make_tracks(&lib, 100);
    let reader = {
        let lib = lib.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                let page = lib.query_tracks(&TrackQuery::default()).unwrap();
                assert!(page.total >= 100);
            }
        })
    };
    for i in 0..50 {
        lib.insert_track(&track(&format!("/w/{i}"), "W", "W"))
            .unwrap();
    }
    reader.join().unwrap();
    assert_eq!(lib.track_count().unwrap(), 150);
    lib.maintenance().unwrap();
}
