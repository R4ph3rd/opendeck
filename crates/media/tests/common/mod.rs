//! Test fixture generators: tiny WAV, AIFF and FLAC (verbatim) writers.
//! Nothing here ships audio; every fixture is synthesised at test time.
#![allow(dead_code, missing_docs)]

use std::path::{Path, PathBuf};

/// Per-test scratch directory under the target dir.
pub fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-media-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Sine at `freq`, amplitude `amp`, as channel-major `Vec<Vec<f32>>`.
pub fn sine(rate: u32, secs: f32, freq: f32, amp: f32, channels: usize) -> Vec<Vec<f32>> {
    let n = (rate as f32 * secs) as usize;
    (0..channels)
        .map(|c| {
            (0..n)
                .map(|i| {
                    amp * (std::f64::consts::TAU * f64::from(freq) * (c as f64 + 1.0) * i as f64
                        / f64::from(rate))
                    .sin() as f32
                })
                .collect()
        })
        .collect()
}

/// Deterministic white noise in `[-amp, amp]` (xorshift).
pub fn noise(rate: u32, secs: f32, amp: f32, channels: usize) -> Vec<Vec<f32>> {
    let n = (rate as f32 * secs) as usize;
    let mut x = 0x2545_F491u32;
    (0..channels)
        .map(|_| {
            (0..n)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    amp * ((x as f32 / u32::MAX as f32) * 2.0 - 1.0)
                })
                .collect()
        })
        .collect()
}

fn q16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// 16-bit PCM WAV.
pub fn wav16(path: &Path, rate: u32, ch: &[Vec<f32>]) {
    let n = ch[0].len();
    let nch = ch.len() as u16;
    let data_len = (n * ch.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend(b"RIFF");
    b.extend((36 + data_len).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(nch.to_le_bytes());
    b.extend(rate.to_le_bytes());
    b.extend((rate * u32::from(nch) * 2).to_le_bytes());
    b.extend((nch * 2).to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data_len.to_le_bytes());
    for i in 0..n {
        for c in ch {
            b.extend(q16(c[i]).to_le_bytes());
        }
    }
    std::fs::write(path, b).unwrap();
}

/// 16-bit PCM AIFF (big endian, 80-bit extended sample rate).
pub fn aiff16(path: &Path, rate: u32, ch: &[Vec<f32>]) {
    let n = ch[0].len();
    let nch = ch.len() as u16;
    let data_len = (n * ch.len() * 2) as u32;
    // 80-bit extended float for an integer rate.
    let exp = 31 - rate.leading_zeros();
    let mant = u64::from(rate) << (63 - exp);
    let mut ext = Vec::new();
    ext.extend(((16383 + exp) as u16).to_be_bytes());
    ext.extend(mant.to_be_bytes());
    let mut b = Vec::new();
    b.extend(b"FORM");
    b.extend((4 + 8 + 18 + 8 + 8 + data_len).to_be_bytes());
    b.extend(b"AIFFCOMM");
    b.extend(18u32.to_be_bytes());
    b.extend(nch.to_be_bytes());
    b.extend((n as u32).to_be_bytes());
    b.extend(16u16.to_be_bytes());
    b.extend(ext);
    b.extend(b"SSND");
    b.extend((8 + data_len).to_be_bytes());
    b.extend(0u32.to_be_bytes());
    b.extend(0u32.to_be_bytes());
    for i in 0..n {
        for c in ch {
            b.extend(q16(c[i]).to_be_bytes());
        }
    }
    std::fs::write(path, b).unwrap();
}

fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &byte in data {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x8005
            } else {
                crc << 1
            };
        }
    }
    crc
}

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u64, bits: u32) {
        self.acc = (self.acc << bits) | (v & ((1u64 << bits) - 1));
        self.n += bits;
        while self.n >= 8 {
            self.out.push((self.acc >> (self.n - 8)) as u8);
            self.n -= 8;
        }
    }
}

/// 16-bit FLAC using verbatim subframes (valid, uncompressed). Block size 4096.
/// `rate` must be representable in the STREAMINFO 20-bit field.
pub fn flac16(path: &Path, rate: u32, ch: &[Vec<f32>]) {
    const BLOCK: usize = 4096;
    let n = ch[0].len();
    let nch = ch.len();
    let mut file = Vec::new();
    file.extend(b"fLaC");
    // STREAMINFO, last-metadata-block flag set.
    file.push(0x80);
    file.extend([0, 0, 34]);
    file.extend((BLOCK as u16).to_be_bytes());
    file.extend((BLOCK as u16).to_be_bytes());
    file.extend([0, 0, 0]);
    file.extend([0, 0, 0]);
    let mut si = Bits {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    si.put(u64::from(rate), 20);
    si.put(nch as u64 - 1, 3);
    si.put(15, 5);
    si.put(n as u64, 36);
    file.extend(si.out);
    file.extend([0u8; 16]);

    let mut frame_no = 0u32;
    let mut pos = 0;
    while pos < n {
        let len = BLOCK.min(n - pos);
        let mut h = Vec::new();
        h.extend([0xFF, 0xF8]);
        // block size code 7 (16-bit value follows), sample rate code 0 (from STREAMINFO)
        h.push(0x70);
        // channel assignment independent (nch-1), sample size 16 bits (100), reserved 0
        h.push((((nch - 1) as u8) << 4) | (0b100 << 1));
        // UTF-8 coded frame number
        if frame_no < 0x80 {
            h.push(frame_no as u8);
        } else if frame_no < 0x800 {
            h.push(0xC0 | (frame_no >> 6) as u8);
            h.push(0x80 | (frame_no & 0x3F) as u8);
        } else {
            h.push(0xE0 | (frame_no >> 12) as u8);
            h.push(0x80 | ((frame_no >> 6) & 0x3F) as u8);
            h.push(0x80 | (frame_no & 0x3F) as u8);
        }
        h.extend(((len - 1) as u16).to_be_bytes());
        h.push(crc8(&h));
        let mut f = h;
        for c in ch {
            f.push(0x02); // verbatim subframe, no wasted bits
            for &s in &c[pos..pos + len] {
                f.extend(q16(s).to_be_bytes());
            }
        }
        let crc = crc16(&f);
        f.extend(crc.to_be_bytes());
        file.extend(f);
        pos += len;
        frame_no += 1;
    }
    std::fs::write(path, file).unwrap();
}

/// Root-mean-square of a slice.
pub fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}
