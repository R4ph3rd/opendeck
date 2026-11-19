//! Small conversions between VST3 types and Rust: strings, class ids,
//! result codes.

use std::ffi::c_char;

use vst3::Steinberg::Vst::TChar;
use vst3::Steinberg::{TUID, kResultOk, tresult};

/// A VST3 enum constant as `i32`. The bindings type enum constants as the
/// C++ compiler's enum type (`u32` on Linux and macOS, `i32` on Windows),
/// while the interfaces take `i32`.
pub(super) fn e(v: impl Into<i64>) -> i32 {
    v.into() as i32
}

/// True for `kResultOk` (`kResultTrue` has the same value).
pub(super) fn is_ok(result: tresult) -> bool {
    result == kResultOk
}

/// UTF-16 up to the first NUL, lossy.
pub(super) fn utf16_to_string(s: &[TChar]) -> String {
    let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

/// 8-bit characters up to the first NUL, lossy UTF-8.
pub(super) fn cchars_to_string(s: &[c_char]) -> String {
    let bytes: Vec<u8> = s
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Writes `s` as NUL-terminated UTF-16, truncating to fit.
pub(super) fn write_utf16(s: &str, dst: &mut [TChar]) {
    let room = dst.len().saturating_sub(1);
    let mut n = 0;
    for (slot, unit) in dst.iter_mut().take(room).zip(s.encode_utf16()) {
        *slot = unit as TChar;
        n += 1;
    }
    if let Some(end) = dst.get_mut(n) {
        *end = 0;
    }
}

/// The class id as 32 uppercase hex digits, the plugin id in `vst3:<id>`
/// keys (ADR 0008).
///
/// The digits are the four 32-bit words of the id as the SDK's `FUID` and
/// the `uid(l1, l2, l3, l4)` helper write them, so the id is the same on
/// every OS. The memory layout differs: Windows uses the COM `GUID` layout
/// (first three fields little endian), the other systems write all four
/// words big endian (`windows` selects the layout of `tuid`).
pub(super) fn tuid_to_hex_for(tuid: &[u8; 16], windows: bool) -> String {
    let [a, b, c, d] = words_from_bytes(tuid, windows);
    format!("{a:08X}{b:08X}{c:08X}{d:08X}")
}

/// Inverse of [`tuid_to_hex_for`]; `None` unless `hex` is 32 hex digits.
pub(super) fn hex_to_tuid_for(hex: &str, windows: bool) -> Option<[u8; 16]> {
    if hex.len() != 32 || !hex.is_ascii() {
        return None;
    }
    let mut words = [0u32; 4];
    for (i, w) in words.iter_mut().enumerate() {
        *w = u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).ok()?;
    }
    Some(words_to_bytes(words, windows))
}

fn words_from_bytes(t: &[u8; 16], windows: bool) -> [u32; 4] {
    let be = |i: usize| u32::from_be_bytes([t[i], t[i + 1], t[i + 2], t[i + 3]]);
    if windows {
        [
            u32::from_le_bytes([t[0], t[1], t[2], t[3]]),
            u32::from(t[4]) << 16 | u32::from(t[5]) << 24 | u32::from(t[6]) | u32::from(t[7]) << 8,
            be(8),
            be(12),
        ]
    } else {
        [be(0), be(4), be(8), be(12)]
    }
}

fn words_to_bytes([a, b, c, d]: [u32; 4], windows: bool) -> [u8; 16] {
    let mut out = [0u8; 16];
    if windows {
        out[0..4].copy_from_slice(&a.to_le_bytes());
        out[4] = (b >> 16) as u8;
        out[5] = (b >> 24) as u8;
        out[6] = b as u8;
        out[7] = (b >> 8) as u8;
    } else {
        out[0..4].copy_from_slice(&a.to_be_bytes());
        out[4..8].copy_from_slice(&b.to_be_bytes());
    }
    out[8..12].copy_from_slice(&c.to_be_bytes());
    out[12..16].copy_from_slice(&d.to_be_bytes());
    out
}

/// Class id hex of a `TUID` as this OS lays it out.
pub(super) fn tuid_to_hex(tuid: &TUID) -> String {
    tuid_to_hex_for(&tuid_bytes(tuid), cfg!(windows))
}

/// `TUID` for a class id hex string, in this OS's layout.
pub(super) fn hex_to_tuid(hex: &str) -> Option<TUID> {
    hex_to_tuid_for(hex, cfg!(windows)).map(|b| b.map(|x| x as c_char))
}

/// The bytes of a `TUID`.
pub(super) fn tuid_bytes(tuid: &TUID) -> [u8; 16] {
    tuid.map(|c| c as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::uid;

    #[test]
    fn class_id_hex_matches_the_uid_helper_on_this_os() {
        let tuid = uid(0x6E33_2252, 0x5422_4A00, 0xAA69_301A, 0xF318_797D);
        assert_eq!(tuid_to_hex(&tuid), "6E33225254224A00AA69301AF318797D");
        assert_eq!(hex_to_tuid("6E33225254224A00AA69301AF318797D"), Some(tuid));
    }

    #[test]
    fn class_id_hex_is_the_same_on_every_os_layout() {
        for windows in [false, true] {
            let bytes = words_to_bytes(
                [0x0102_0304, 0x0506_0708, 0x090A_0B0C, 0x0D0E_0F10],
                windows,
            );
            assert_eq!(
                tuid_to_hex_for(&bytes, windows),
                "0102030405060708090A0B0C0D0E0F10"
            );
            assert_eq!(
                hex_to_tuid_for("0102030405060708090A0B0C0D0E0F10", windows),
                Some(bytes)
            );
        }
        // The COM layout of 01020304 05060708: `04 03 02 01 06 05 08 07`.
        let win = hex_to_tuid_for("0102030405060708090A0B0C0D0E0F10", true).unwrap();
        assert_eq!(win[..8], [4, 3, 2, 1, 6, 5, 8, 7]);
        assert!(hex_to_tuid_for("xyz", false).is_none());
    }

    #[test]
    fn strings_round_trip_and_truncate() {
        let mut buf = [1 as TChar; 8];
        write_utf16("Gain é", &mut buf);
        assert_eq!(utf16_to_string(&buf), "Gain é");
        write_utf16("much too long", &mut buf);
        assert_eq!(utf16_to_string(&buf), "much to");
        let c: Vec<c_char> = b"Fx|Reverb\0junk".iter().map(|b| *b as c_char).collect();
        assert_eq!(cchars_to_string(&c), "Fx|Reverb");
    }
}
