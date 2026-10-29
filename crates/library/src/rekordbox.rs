//! rekordbox XML collection export reader (`DJ_PLAYLISTS`).
//!
//! Source: Pioneer DJ "rekordbox XML Format List" and the documented export
//! (File > Export Collection in xml format). Only that documented format is
//! read; no database file and no third-party exports are interpreted.
//!
//! Layout used here:
//!
//! ```text
//! <DJ_PLAYLISTS Version="1.0.0">
//!   <COLLECTION Entries="n">
//!     <TRACK TrackID Name Artist Album Genre Comments Year TotalTime AverageBpm
//!            Tonality Rating Location ...>
//!       <TEMPO Inizio Bpm Metro Battito/>         beat grid markers
//!       <POSITION_MARK Name Type Start End Num Red Green Blue/>   cues
//!     </TRACK>
//!   </COLLECTION>
//!   <PLAYLISTS>
//!     <NODE Type="0" Name="ROOT">                 Type 0 folder, 1 playlist
//!       <NODE Type="1" Name="Set" KeyType="0"><TRACK Key="1"/></NODE>
//!     </NODE>
//!   </PLAYLISTS>
//! </DJ_PLAYLISTS>
//! ```
//!
//! Positions (`Inizio`, `Start`, `End`) are seconds, matching ours.
//!
//! TODO(verify) against the Pioneer PDF: `Type` codes of `POSITION_MARK`
//! (0 cue, 1 fade-in, 2 fade-out, 3 load, 4 loop), `Num` (-1 memory, 0 and up
//! hot cue slot), `Rating` scale (0 to 255 in steps of 51), `Battito` as the
//! beat number (1 to 4) of the marker inside its bar.

use std::path::PathBuf;

use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::cue::{Cue, CueColor, CueKind, HotCueIndex};
use opendeck_contracts::units::{Bpm, Seconds};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::error::{LibraryError, Result};

/// Beat grid marker (`TEMPO`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RbTempo {
    /// Marker position, seconds.
    pub inizio: f64,
    /// Tempo from this marker on.
    pub bpm: f64,
    /// Beats per bar (numerator of `Metro`).
    pub beats_per_bar: u8,
    /// Beat number inside the bar at `inizio`, 1-based.
    pub battito: u8,
}

/// Kind of a `POSITION_MARK`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RbMarkKind {
    /// Cue point (0).
    Cue,
    /// Fade-in (1), ignored on import.
    FadeIn,
    /// Fade-out (2), ignored on import.
    FadeOut,
    /// Load marker (3), ignored on import.
    Load,
    /// Loop (4).
    Loop,
}

/// A `POSITION_MARK`.
#[derive(Debug, Clone, PartialEq)]
pub struct RbMark {
    /// Label.
    pub name: String,
    /// Kind.
    pub kind: RbMarkKind,
    /// Slot: -1 memory cue, 0 and up hot cue.
    pub num: i32,
    /// Start, seconds.
    pub start: f64,
    /// Loop end, seconds.
    pub end: Option<f64>,
    /// Colour, when the mark has one.
    pub color: Option<CueColor>,
}

/// One `COLLECTION/TRACK`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RbTrack {
    /// `TrackID`, referenced by playlists.
    pub track_id: String,
    /// File path decoded from the `Location` URL.
    pub location: PathBuf,
    /// Title.
    pub title: Option<String>,
    /// Artist.
    pub artist: Option<String>,
    /// Album.
    pub album: Option<String>,
    /// Genre.
    pub genre: Option<String>,
    /// Comments.
    pub comment: Option<String>,
    /// Remixer.
    pub remixer: Option<String>,
    /// Label.
    pub label: Option<String>,
    /// Year.
    pub year: Option<i32>,
    /// `AverageBpm`.
    pub average_bpm: Option<f64>,
    /// `Tonality` as written.
    pub tonality: Option<String>,
    /// Rating mapped to 0 to 5.
    pub rating: Option<u8>,
    /// Beat grid markers in file order.
    pub tempos: Vec<RbTempo>,
    /// Position marks in file order.
    pub marks: Vec<RbMark>,
}

impl RbTrack {
    /// Constant-tempo grid from the first `TEMPO` marker.
    ///
    /// Our grid (contracts v1) is one tempo plus a downbeat anchor, so a
    /// variable-tempo track keeps only its first segment (see
    /// [`RbTrack::is_variable_tempo`]). The anchor is moved back from the
    /// marker to the bar's downbeat using `Battito`.
    pub fn grid(&self) -> Option<BeatGrid> {
        let t = self.tempos.first()?;
        let beat = 60.0 / t.bpm;
        let back = f64::from(t.battito.saturating_sub(1)) * beat;
        BeatGrid::new(Seconds(t.inizio - back), Bpm(t.bpm), t.beats_per_bar).ok()
    }

    /// More than one distinct tempo: the grid is only valid for the first segment.
    pub fn is_variable_tempo(&self) -> bool {
        self.tempos
            .iter()
            .any(|t| (t.bpm - self.tempos[0].bpm).abs() > 0.005)
    }

    /// Marks mapped to our cues: slots 0 to 7 become hot cues, -1 memory cues,
    /// a loop without slot a saved loop. Fades, load markers and out-of-range
    /// slots are skipped.
    pub fn cues(&self) -> Vec<Cue> {
        self.marks
            .iter()
            .filter_map(|m| {
                let kind = match (m.kind, m.num) {
                    (RbMarkKind::Cue, -1) => CueKind::Memory,
                    (RbMarkKind::Loop, -1) => CueKind::Loop,
                    (RbMarkKind::Cue | RbMarkKind::Loop, n) if n >= 0 => CueKind::Hot {
                        index: HotCueIndex::new(u8::try_from(n).ok()?)?,
                    },
                    _ => return None,
                };
                let end = match m.kind {
                    RbMarkKind::Loop => Some(Seconds(m.end?)),
                    _ => None,
                };
                let cue = Cue {
                    id: None,
                    kind,
                    start: Seconds(m.start),
                    end,
                    color: m.color,
                    name: m.name.clone(),
                };
                cue.is_valid().then_some(cue)
            })
            .collect()
    }
}

/// How a playlist entry points at a track.
#[derive(Debug, Clone, PartialEq)]
pub enum RbEntry {
    /// `KeyType="0"`: a `TrackID`.
    Id(String),
    /// `KeyType="1"`: a file location.
    Location(PathBuf),
}

/// `PLAYLISTS/NODE`.
#[derive(Debug, Clone, PartialEq)]
pub enum RbNode {
    /// `Type="0"`.
    Folder {
        /// Name.
        name: String,
        /// Child nodes.
        children: Vec<RbNode>,
    },
    /// `Type="1"`.
    Playlist {
        /// Name.
        name: String,
        /// Entries in order.
        entries: Vec<RbEntry>,
    },
}

/// Parsed export.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RekordboxLibrary {
    /// Collection tracks.
    pub tracks: Vec<RbTrack>,
    /// Children of the `ROOT` node.
    pub playlists: Vec<RbNode>,
}

fn parse_err(e: impl std::fmt::Display) -> LibraryError {
    LibraryError::Parse(format!("rekordbox xml: {e}"))
}

/// Decodes `%XX` escapes (UTF-8).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Converts a rekordbox `Location` (`file://localhost/C:/Music/a%20b.mp3`) to a path.
pub fn location_to_path(location: &str) -> PathBuf {
    let rest = location
        .strip_prefix("file://localhost")
        .or_else(|| location.strip_prefix("file://"))
        .unwrap_or(location);
    let decoded = percent_decode(rest);
    // "/C:/Music" is a Windows drive path: drop the leading slash.
    let b = decoded.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        return PathBuf::from(&decoded[1..]);
    }
    PathBuf::from(decoded)
}

fn attrs(e: &BytesStart<'_>) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(parse_err)?;
        let key = a.key.as_ref().to_owned();
        let val = a
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(parse_err)?
            .into_owned();
        out.push((key, val));
    }
    Ok(out)
}

fn get<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

fn num<T: std::str::FromStr>(attrs: &[(String, String)], key: &str) -> Option<T> {
    get(attrs, key).and_then(|v| v.trim().parse().ok())
}

fn track_from(a: &[(String, String)]) -> RbTrack {
    RbTrack {
        track_id: get(a, "TrackID").unwrap_or_default().to_owned(),
        location: get(a, "Location").map(location_to_path).unwrap_or_default(),
        title: get(a, "Name").map(str::to_owned),
        artist: get(a, "Artist").map(str::to_owned),
        album: get(a, "Album").map(str::to_owned),
        genre: get(a, "Genre").map(str::to_owned),
        comment: get(a, "Comments").map(str::to_owned),
        remixer: get(a, "Remixer").map(str::to_owned),
        label: get(a, "Label").map(str::to_owned),
        year: num::<i32>(a, "Year").filter(|y| *y > 0),
        average_bpm: num::<f64>(a, "AverageBpm").filter(|b| *b > 0.0),
        tonality: get(a, "Tonality").map(str::to_owned),
        rating: num::<u32>(a, "Rating").map(|r| (r / 51).min(5) as u8),
        tempos: Vec::new(),
        marks: Vec::new(),
    }
}

fn tempo_from(a: &[(String, String)]) -> Option<RbTempo> {
    let bpm: f64 = num(a, "Bpm")?;
    if !(bpm.is_finite() && bpm > 0.0) {
        return None;
    }
    let beats_per_bar = get(a, "Metro")
        .and_then(|m| m.split('/').next())
        .and_then(|n| n.trim().parse::<u8>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(4);
    Some(RbTempo {
        inizio: num(a, "Inizio")?,
        bpm,
        beats_per_bar,
        battito: num::<u8>(a, "Battito").filter(|b| *b >= 1).unwrap_or(1),
    })
}

fn mark_from(a: &[(String, String)]) -> Option<RbMark> {
    let kind = match num::<i32>(a, "Type")? {
        0 => RbMarkKind::Cue,
        1 => RbMarkKind::FadeIn,
        2 => RbMarkKind::FadeOut,
        3 => RbMarkKind::Load,
        4 => RbMarkKind::Loop,
        _ => return None,
    };
    let color = match (
        num::<u8>(a, "Red"),
        num::<u8>(a, "Green"),
        num::<u8>(a, "Blue"),
    ) {
        (Some(r), Some(g), Some(b)) => Some(CueColor { r, g, b }),
        _ => None,
    };
    Some(RbMark {
        name: get(a, "Name").unwrap_or_default().to_owned(),
        kind,
        num: num(a, "Num").unwrap_or(-1),
        start: num(a, "Start")?,
        end: num(a, "End"),
        color,
    })
}

/// Playlist node under construction.
struct Frame {
    name: String,
    is_folder: bool,
    key_is_location: bool,
    children: Vec<RbNode>,
    entries: Vec<RbEntry>,
}

/// Parses a rekordbox XML export.
pub fn parse(xml: &str) -> Result<RekordboxLibrary> {
    #[derive(PartialEq)]
    enum Section {
        None,
        Collection,
        Playlists,
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = RekordboxLibrary::default();
    let mut section = Section::None;
    let mut current: Option<RbTrack> = None;
    let mut stack: Vec<Frame> = Vec::new();
    let mut saw_root = false;

    loop {
        let ev = reader.read_event().map_err(parse_err)?;
        let (start, is_empty) = match &ev {
            Event::Start(e) => (Some(e), false),
            Event::Empty(e) => (Some(e), true),
            _ => (None, false),
        };
        if let Some(e) = start {
            let name = e.name().as_ref().to_owned();
            let a = attrs(e)?;
            match name.as_str() {
                "DJ_PLAYLISTS" => saw_root = true,
                "COLLECTION" => section = Section::Collection,
                "PLAYLISTS" => section = Section::Playlists,
                "TRACK" if section == Section::Collection => {
                    let t = track_from(&a);
                    if is_empty {
                        out.tracks.push(t);
                    } else {
                        current = Some(t);
                    }
                }
                "TEMPO" => {
                    if let (Some(t), Some(tempo)) = (current.as_mut(), tempo_from(&a)) {
                        t.tempos.push(tempo);
                    }
                }
                "POSITION_MARK" => {
                    if let (Some(t), Some(mark)) = (current.as_mut(), mark_from(&a)) {
                        t.marks.push(mark);
                    }
                }
                "NODE" if section == Section::Playlists => {
                    let frame = Frame {
                        name: get(&a, "Name").unwrap_or_default().to_owned(),
                        is_folder: get(&a, "Type") == Some("0"),
                        key_is_location: get(&a, "KeyType") == Some("1"),
                        children: Vec::new(),
                        entries: Vec::new(),
                    };
                    if is_empty {
                        close_node(frame, &mut stack, &mut out);
                    } else {
                        stack.push(frame);
                    }
                }
                "TRACK" if section == Section::Playlists => {
                    if let (Some(f), Some(key)) = (stack.last_mut(), get(&a, "Key")) {
                        f.entries.push(if f.key_is_location {
                            RbEntry::Location(location_to_path(key))
                        } else {
                            RbEntry::Id(key.to_owned())
                        });
                    }
                }
                _ => {}
            }
        }
        match ev {
            Event::End(e) => match e.name().as_ref() {
                "TRACK" if section == Section::Collection => {
                    if let Some(t) = current.take() {
                        out.tracks.push(t);
                    }
                }
                "COLLECTION" | "PLAYLISTS" => section = Section::None,
                "NODE" if section == Section::Playlists => {
                    if let Some(f) = stack.pop() {
                        close_node(f, &mut stack, &mut out);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    if !saw_root {
        return Err(LibraryError::Parse(
            "rekordbox xml: no DJ_PLAYLISTS element".into(),
        ));
    }
    Ok(out)
}

/// Attaches a finished node to its parent; the outermost `ROOT` folder
/// dissolves into the top level.
fn close_node(f: Frame, stack: &mut [Frame], out: &mut RekordboxLibrary) {
    if stack.is_empty() && f.is_folder && f.name == "ROOT" {
        out.playlists.extend(f.children);
        return;
    }
    let node = if f.is_folder {
        RbNode::Folder {
            name: f.name,
            children: f.children,
        }
    } else {
        RbNode::Playlist {
            name: f.name,
            entries: f.entries,
        }
    };
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => out.playlists.push(node),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations_decode() {
        assert_eq!(
            location_to_path("file://localhost/Users/dj/My%20Music/a%26b.mp3"),
            PathBuf::from("/Users/dj/My Music/a&b.mp3")
        );
        assert_eq!(
            location_to_path("file://localhost/C:/Music/x.wav"),
            PathBuf::from("C:/Music/x.wav")
        );
        assert_eq!(
            location_to_path("/plain/path.wav"),
            PathBuf::from("/plain/path.wav")
        );
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%41"), "%zzA");
    }

    #[test]
    fn rejects_non_rekordbox_xml() {
        assert!(parse("<html/>").is_err());
        assert!(parse("<DJ_PLAYLISTS><COLLECTION></DJ_PLAYLISTS>").is_err());
    }
}
