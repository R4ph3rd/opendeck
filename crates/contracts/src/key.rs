//! Musical key with Camelot and musical notation.
//!
//! Camelot wheel: major keys are `1B..12B`, minor keys `1A..12A`, with C major
//! at `8B` and A minor at `8A`. Adjacent numbers are a fifth apart; the same
//! number with the other letter is the relative major/minor.

use core::fmt;

/// Major or minor mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum KeyMode {
    /// Major (Camelot letter B).
    Major,
    /// Minor (Camelot letter A).
    Minor,
}

/// A musical key: tonic pitch class plus mode. RT-safe.
///
/// Invariant: `tonic < 12`, where 0 = C, 1 = C#/Db, ..., 11 = B. Build with
/// [`MusicalKey::new`] or a parser. Serialized as the Camelot string (`"8A"`),
/// which is also the format stored in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "String", into = "String"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MusicalKey {
    tonic: u8,
    mode: KeyMode,
}

/// Pitch class names used for musical notation. Flats for black keys except
/// F#, the common DJ software convention.
const PITCH_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// Error returned when a key string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyParseError;

impl fmt::Display for KeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a Camelot (e.g. 8A) or musical (e.g. Am, F#) key")
    }
}

impl std::error::Error for KeyParseError {}

impl MusicalKey {
    /// Returns the key, or `None` if `tonic >= 12`.
    pub const fn new(tonic: u8, mode: KeyMode) -> Option<Self> {
        if tonic < 12 {
            Some(Self { tonic, mode })
        } else {
            None
        }
    }

    /// Tonic pitch class, `0..12` with 0 = C.
    pub const fn tonic(self) -> u8 {
        self.tonic
    }

    /// Major or minor.
    pub const fn mode(self) -> KeyMode {
        self.mode
    }

    /// Camelot number, `1..=12`.
    pub const fn camelot_number(self) -> u8 {
        // Relative major of a minor key is three semitones up.
        let major_tonic = match self.mode {
            KeyMode::Major => self.tonic,
            KeyMode::Minor => (self.tonic + 3) % 12,
        };
        // Each fifth (7 semitones) is one step; C major is 8.
        ((major_tonic as u16 * 7 + 7) % 12) as u8 + 1
    }

    /// Camelot letter: `'B'` for major, `'A'` for minor.
    pub const fn camelot_letter(self) -> char {
        match self.mode {
            KeyMode::Major => 'B',
            KeyMode::Minor => 'A',
        }
    }

    /// Camelot notation, e.g. `"8A"`.
    pub fn camelot(self) -> String {
        format!("{}{}", self.camelot_number(), self.camelot_letter())
    }

    /// Musical notation, e.g. `"Am"`, `"F#"`, `"Bbm"`.
    pub fn musical(self) -> String {
        let name = PITCH_NAMES[self.tonic as usize];
        match self.mode {
            KeyMode::Major => name.to_owned(),
            KeyMode::Minor => format!("{name}m"),
        }
    }

    /// Parses Camelot notation (`"8A"`, `"12b"`).
    pub fn from_camelot(text: &str) -> Result<Self, KeyParseError> {
        let text = text.trim();
        let letter = text.chars().next_back().ok_or(KeyParseError)?;
        let num = &text[..text.len() - letter.len_utf8()];
        let number: u8 = num.parse().map_err(|_| KeyParseError)?;
        if !(1..=12).contains(&number) {
            return Err(KeyParseError);
        }
        let mode = match letter {
            'A' | 'a' => KeyMode::Minor,
            'B' | 'b' => KeyMode::Major,
            _ => return Err(KeyParseError),
        };
        // Invert camelot_number: major tonic = (number - 8) * 7 mod 12 (7 is its own inverse mod 12).
        let major_tonic = ((u16::from(number) + 12 - 8) * 7 % 12) as u8;
        let tonic = match mode {
            KeyMode::Major => major_tonic,
            KeyMode::Minor => (major_tonic + 9) % 12,
        };
        Ok(Self { tonic, mode })
    }

    /// Parses musical notation: a letter `A..G`, optional `#` or `b`, and an
    /// optional minor suffix `m` or `min` (`"Am"`, `"F#"`, `"Ebm"`).
    pub fn from_musical(text: &str) -> Result<Self, KeyParseError> {
        let text = text.trim();
        let mut chars = text.chars();
        let base: i16 = match chars.next().map(|c| c.to_ascii_uppercase()) {
            Some('C') => 0,
            Some('D') => 2,
            Some('E') => 4,
            Some('F') => 5,
            Some('G') => 7,
            Some('A') => 9,
            Some('B') => 11,
            _ => return Err(KeyParseError),
        };
        let mut rest = chars.as_str();
        let mut tonic = base;
        if let Some(stripped) = rest.strip_prefix('#') {
            tonic += 1;
            rest = stripped;
        } else if let Some(stripped) = rest.strip_prefix('b') {
            tonic -= 1;
            rest = stripped;
        }
        let mode = match rest {
            "" => KeyMode::Major,
            "m" | "min" => KeyMode::Minor,
            _ => return Err(KeyParseError),
        };
        Ok(Self {
            tonic: tonic.rem_euclid(12) as u8,
            mode,
        })
    }

    /// Parses either notation.
    pub fn parse(text: &str) -> Result<Self, KeyParseError> {
        Self::from_camelot(text).or_else(|_| Self::from_musical(text))
    }

    /// Transposes by `semitones` (positive = up), keeping the mode.
    pub const fn transposed(self, semitones: i8) -> Self {
        let tonic = (self.tonic as i16 + semitones as i16).rem_euclid(12) as u8;
        Self {
            tonic,
            mode: self.mode,
        }
    }
}

impl fmt::Display for MusicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.camelot_number(), self.camelot_letter())
    }
}

impl TryFrom<String> for MusicalKey {
    type Error = KeyParseError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::from_camelot(&text)
    }
}

impl From<MusicalKey> for String {
    fn from(key: MusicalKey) -> Self {
        key.camelot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> MusicalKey {
        MusicalKey::parse(text).expect("valid key")
    }

    #[test]
    fn camelot_reference_points() {
        assert_eq!(key("C").camelot(), "8B");
        assert_eq!(key("Am").camelot(), "8A");
        assert_eq!(key("G").camelot(), "9B");
        assert_eq!(key("Em").camelot(), "9A");
        assert_eq!(key("F").camelot(), "7B");
        assert_eq!(key("B").camelot(), "1B");
        assert_eq!(key("G#m").camelot(), "1A");
        assert_eq!(key("Ebm").camelot(), "2A");
        assert_eq!(key("Db").camelot(), "3B");
    }

    #[test]
    fn camelot_round_trip_all_24() {
        for tonic in 0..12 {
            for mode in [KeyMode::Major, KeyMode::Minor] {
                let k = MusicalKey::new(tonic, mode).expect("tonic < 12");
                assert_eq!(MusicalKey::from_camelot(&k.camelot()), Ok(k));
                assert_eq!(MusicalKey::from_musical(&k.musical()), Ok(k));
            }
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(MusicalKey::parse("13A").is_err());
        assert!(MusicalKey::parse("H").is_err());
        assert!(MusicalKey::parse("").is_err());
        assert!(MusicalKey::new(12, KeyMode::Major).is_none());
    }

    #[test]
    fn transpose_wraps() {
        assert_eq!(key("B").transposed(1), key("C"));
        assert_eq!(key("C").transposed(-1), key("B"));
        assert_eq!(key("Am").transposed(12), key("Am"));
    }
}
