//! Tag and artwork reading (lofty).

use std::path::Path;
use std::time::Duration;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::tag::Accessor;

use crate::error::MediaError;

/// Embedded cover art.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    /// MIME type, e.g. `image/jpeg`.
    pub mime: String,
    /// Raw image bytes.
    pub data: Vec<u8>,
}

/// Metadata read from a file. Missing fields are `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackTags {
    /// Title.
    pub title: Option<String>,
    /// Artist.
    pub artist: Option<String>,
    /// Album.
    pub album: Option<String>,
    /// Genre.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<u32>,
    /// Comment.
    pub comment: Option<String>,
    /// Duration.
    pub duration: Duration,
    /// Overall bitrate, kbit/s.
    pub bitrate_kbps: Option<u32>,
    /// Sample rate, Hz.
    pub sample_rate: Option<u32>,
    /// Channel count.
    pub channels: Option<u8>,
    /// Front cover, or the first picture when no front cover exists.
    pub artwork: Option<Artwork>,
}

/// Reads tags, audio properties and artwork from `path`.
pub fn read_tags(path: &Path) -> Result<TrackTags, MediaError> {
    let file = lofty::read_from_path(path).map_err(|e| MediaError::Tags(e.to_string()))?;
    let props = file.properties();
    let mut out = TrackTags {
        duration: props.duration(),
        bitrate_kbps: props.overall_bitrate(),
        sample_rate: props.sample_rate(),
        channels: props.channels(),
        ..TrackTags::default()
    };
    if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
        out.title = tag.title().map(|s| s.into_owned());
        out.artist = tag.artist().map(|s| s.into_owned());
        out.album = tag.album().map(|s| s.into_owned());
        out.genre = tag.genre().map(|s| s.into_owned());
        out.comment = tag.comment().map(|s| s.into_owned());
        out.year = tag.date().map(|d| u32::from(d.year));
        let pic = tag
            .pictures()
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| tag.pictures().first());
        out.artwork = pic.map(|p| Artwork {
            mime: p
                .mime_type()
                .map_or_else(|| "application/octet-stream".into(), |m| m.to_string()),
            data: p.data().to_vec(),
        });
    }
    Ok(out)
}
