//! Waveform peak blob format, version 1.
//!
//! Peaks are computed once at analysis, stored per zoom level in the
//! `analysis_peaks` table, and sent to the UI as raw bytes over
//! `tauri::ipc::Response` (ADR 0004). The same byte layout is used in the DB
//! and on the wire, so the UI decodes exactly what the library stored.
//!
//! Layout (all integers little-endian):
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0 | 4 | magic `b"ODPK"` |
//! | 4 | 2 | format version (`1`) |
//! | 6 | 2 | bytes per point (`4`) |
//! | 8 | 4 | sample rate of the analysed audio, Hz |
//! | 12 | 4 | frames per point at that rate |
//! | 16 | 4 | point count |
//! | 20 | 4 | reserved, `0` |
//! | 24 | 4 x count | points: `peak, low, mid, high` as `u8` |
//!
//! `peak` is the absolute sample peak of the stereo-summed signal over the
//! point, scaled `0..=255` for `0.0..=1.0` (clipped above). `low`, `mid`,
//! `high` are band levels for frequency colouring, same scale. Band crossover
//! frequencies are chosen by the analysis crate and documented there; they
//! do not change the layout.

/// Magic bytes at the start of every peak blob.
pub const PEAK_MAGIC: [u8; 4] = *b"ODPK";

/// Current peak blob format version.
pub const PEAK_FORMAT_VERSION: u16 = 1;

/// Header size in bytes.
pub const PEAK_HEADER_LEN: usize = 24;

/// Bytes per point in format version 1.
pub const PEAK_POINT_LEN: usize = 4;

/// One waveform point. 8-bit levels, see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PeakPoint {
    /// Absolute peak of the full-band signal.
    pub peak: u8,
    /// Low band level.
    pub low: u8,
    /// Mid band level.
    pub mid: u8,
    /// High band level.
    pub high: u8,
}

/// Peaks at one zoom level.
///
/// Invariants: `sample_rate > 0`, `frames_per_point > 0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeakLevel {
    /// Rate of the analysed audio.
    pub sample_rate: u32,
    /// Frames summarised by each point.
    pub frames_per_point: u32,
    /// Points from the start of the track.
    pub points: Vec<PeakPoint>,
}

/// Error decoding a peak blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeakDecodeError {
    /// Shorter than the header or than the declared point count.
    Truncated,
    /// Magic bytes do not match.
    BadMagic,
    /// Unknown format version or point size.
    UnsupportedVersion(u16),
    /// Zero sample rate or frames per point.
    Invalid,
}

impl core::fmt::Display for PeakDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("peak blob truncated"),
            Self::BadMagic => f.write_str("not a peak blob"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported peak format version {v}"),
            Self::Invalid => f.write_str("invalid peak blob header"),
        }
    }
}

impl std::error::Error for PeakDecodeError {}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut b = [0u8; 4];
    b.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(b)
}

impl PeakLevel {
    /// Serializes to the version 1 blob layout.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PEAK_HEADER_LEN + self.points.len() * PEAK_POINT_LEN);
        out.extend_from_slice(&PEAK_MAGIC);
        out.extend_from_slice(&PEAK_FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(PEAK_POINT_LEN as u16).to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&self.frames_per_point.to_le_bytes());
        let count = u32::try_from(self.points.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for p in self.points.iter().take(count as usize) {
            out.extend_from_slice(&[p.peak, p.low, p.mid, p.high]);
        }
        out
    }

    /// Parses a version 1 blob.
    pub fn decode(bytes: &[u8]) -> Result<Self, PeakDecodeError> {
        if bytes.len() < PEAK_HEADER_LEN {
            return Err(PeakDecodeError::Truncated);
        }
        if bytes[0..4] != PEAK_MAGIC {
            return Err(PeakDecodeError::BadMagic);
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let point_len = u16::from_le_bytes([bytes[6], bytes[7]]);
        if version != PEAK_FORMAT_VERSION || usize::from(point_len) != PEAK_POINT_LEN {
            return Err(PeakDecodeError::UnsupportedVersion(version));
        }
        let sample_rate = u32_at(bytes, 8);
        let frames_per_point = u32_at(bytes, 12);
        let count = u32_at(bytes, 16) as usize;
        if sample_rate == 0 || frames_per_point == 0 {
            return Err(PeakDecodeError::Invalid);
        }
        let body = &bytes[PEAK_HEADER_LEN..];
        if body.len() < count.saturating_mul(PEAK_POINT_LEN) {
            return Err(PeakDecodeError::Truncated);
        }
        let points = body
            .chunks_exact(PEAK_POINT_LEN)
            .take(count)
            .map(|c| PeakPoint {
                peak: c[0],
                low: c[1],
                mid: c[2],
                high: c[3],
            })
            .collect();
        Ok(Self {
            sample_rate,
            frames_per_point,
            points,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level() -> PeakLevel {
        PeakLevel {
            sample_rate: 44_100,
            frames_per_point: 256,
            points: vec![
                PeakPoint {
                    peak: 255,
                    low: 10,
                    mid: 20,
                    high: 30,
                },
                PeakPoint::default(),
            ],
        }
    }

    #[test]
    fn round_trip() {
        let blob = level().encode();
        assert_eq!(blob.len(), PEAK_HEADER_LEN + 2 * PEAK_POINT_LEN);
        assert_eq!(&blob[0..4], b"ODPK");
        assert_eq!(PeakLevel::decode(&blob), Ok(level()));
    }

    #[test]
    fn rejects_bad_input() {
        let blob = level().encode();
        assert_eq!(
            PeakLevel::decode(&blob[..10]),
            Err(PeakDecodeError::Truncated)
        );
        assert_eq!(
            PeakLevel::decode(&blob[..blob.len() - 1]),
            Err(PeakDecodeError::Truncated)
        );
        let mut bad = blob.clone();
        bad[0] = b'X';
        assert_eq!(PeakLevel::decode(&bad), Err(PeakDecodeError::BadMagic));
        let mut v2 = blob;
        v2[4] = 2;
        assert_eq!(
            PeakLevel::decode(&v2),
            Err(PeakDecodeError::UnsupportedVersion(2))
        );
    }
}
