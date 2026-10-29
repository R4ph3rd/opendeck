//! Tag and artwork reading on a synthesised FLAC.
#![allow(missing_docs)]

mod common;

use common::*;
use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::{Accessor, Tag, TagExt, TagType};
use opendeck_media::{MediaError, read_tags};

#[test]
fn reads_vorbis_comments_and_cover() {
    let dir = tmp("tags");
    let p = dir.join("t.flac");
    flac16(&p, 44_100, &sine(44_100, 1.0, 440.0, 0.5, 2));

    let mut tag = Tag::new(TagType::VorbisComments);
    tag.set_title("Test Title".into());
    tag.set_artist("Test Artist".into());
    tag.set_album("Test Album".into());
    tag.set_genre("House".into());
    tag.set_comment("hello".into());
    tag.insert_text(lofty::tag::ItemKey::RecordingDate, "2021".into());
    let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4];
    tag.push_picture(
        Picture::unchecked(png.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Png)
            .build(),
    );
    tag.save_to_path(&p, WriteOptions::default()).unwrap();
    // Sanity: lofty sees the tag it just wrote.
    assert!(lofty::read_from_path(&p).unwrap().primary_tag().is_some());

    let t = read_tags(&p).unwrap();
    assert_eq!(t.title.as_deref(), Some("Test Title"));
    assert_eq!(t.artist.as_deref(), Some("Test Artist"));
    assert_eq!(t.album.as_deref(), Some("Test Album"));
    assert_eq!(t.genre.as_deref(), Some("House"));
    assert_eq!(t.comment.as_deref(), Some("hello"));
    assert_eq!(t.year, Some(2021));
    assert_eq!(t.sample_rate, Some(44_100));
    assert_eq!(t.channels, Some(2));
    assert!((t.duration.as_secs_f64() - 1.0).abs() < 0.01);
    let art = t.artwork.unwrap();
    assert_eq!(art.mime, "image/png");
    assert_eq!(art.data, png);
}

#[test]
fn untagged_file_and_garbage() {
    let dir = tmp("tags2");
    let p = dir.join("u.wav");
    wav16(&p, 48_000, &sine(48_000, 0.5, 440.0, 0.5, 2));
    let t = read_tags(&p).unwrap();
    assert_eq!(t.title, None);
    assert_eq!(t.sample_rate, Some(48_000));

    let g = dir.join("g.mp3");
    std::fs::write(&g, [7u8; 512]).unwrap();
    assert!(matches!(read_tags(&g), Err(MediaError::Tags(_))));
    assert!(read_tags(&dir.join("nope.flac")).is_err());
}
