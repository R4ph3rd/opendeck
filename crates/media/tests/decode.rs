//! Decode, resample and robustness tests on synthesised fixtures.
#![allow(missing_docs)]

mod common;

use std::time::Instant;

use common::*;
use opendeck_contracts::SampleRate;
use opendeck_contracts::analysis::CancelFlag;
use opendeck_media::{MediaError, decode_file};

const R48: SampleRate = SampleRate(48_000);

fn expected_frames(src: usize, from: u32, to: u32) -> usize {
    (src as f64 * f64::from(to) / f64::from(from)).round() as usize
}

#[test]
fn wav_resample_length_and_tone() {
    let dir = tmp("wav");
    let p = dir.join("a.wav");
    let src = sine(44_100, 2.0, 440.0, 0.5, 2);
    wav16(&p, 44_100, &src);
    let d = decode_file(&p, R48, &CancelFlag::new()).unwrap();
    assert_eq!(d.sample_rate, R48);
    assert_eq!(d.source_rate, SampleRate(44_100));
    assert_eq!(d.source_channels, 2);
    assert_eq!(d.samples.len(), d.frames * 2);
    let want = expected_frames(src[0].len(), 44_100, 48_000);
    assert!(d.frames.abs_diff(want) <= 1, "{} vs {want}", d.frames);

    // Left channel matches a 48 kHz reference sine away from the edges. The
    // FFT resampler's delay trim leaves a constant ~0.5 output frame lead
    // (about 10 us), so the reference is shifted by half a frame.
    let left: Vec<f32> = d.view().channel(0).collect();
    let (a, b) = (4_800, left.len() - 4_800);
    let err: Vec<f32> = (a..b)
        .map(|i| {
            let t = (i as f64 + 0.5) / 48_000.0;
            left[i] - 0.5 * (std::f64::consts::TAU * 440.0 * t).sin() as f32
        })
        .collect();
    assert!(rms(&err) < 0.002, "residual rms {}", rms(&err));
}

#[test]
fn passthrough_when_rate_matches() {
    let dir = tmp("pass");
    let p = dir.join("a.wav");
    let src = sine(48_000, 0.5, 1000.0, 0.5, 2);
    wav16(&p, 48_000, &src);
    let d = decode_file(&p, R48, &CancelFlag::new()).unwrap();
    assert_eq!(d.frames, src[0].len());
    for (i, f) in d.samples.chunks_exact(2).enumerate() {
        assert!((f[0] - src[0][i]).abs() < 2.0 / 32768.0);
        assert!((f[1] - src[1][i]).abs() < 2.0 / 32768.0);
    }
}

#[test]
fn mono_is_duplicated() {
    let dir = tmp("mono");
    let p = dir.join("m.wav");
    wav16(&p, 48_000, &sine(48_000, 0.25, 500.0, 0.4, 1));
    let d = decode_file(&p, R48, &CancelFlag::new()).unwrap();
    assert_eq!(d.source_channels, 1);
    assert!(d.samples.chunks_exact(2).all(|f| f[0] == f[1]));
}

#[test]
fn downsample_and_odd_rates_keep_length() {
    let dir = tmp("rates");
    for (from, to) in [(96_000u32, 48_000u32), (22_050, 48_000), (48_000, 44_100)] {
        let p = dir.join(format!("{from}.wav"));
        let src = noise(from, 1.0, 0.3, 2);
        wav16(&p, from, &src);
        let d = decode_file(&p, SampleRate(to), &CancelFlag::new()).unwrap();
        let want = expected_frames(src[0].len(), from, to);
        assert!(
            d.frames.abs_diff(want) <= 1,
            "{from}->{to}: {} vs {want}",
            d.frames
        );
    }
}

#[test]
fn flac_and_aiff_decode() {
    let dir = tmp("fmt");
    let src = sine(44_100, 1.0, 330.0, 0.5, 2);
    let f = dir.join("a.flac");
    let a = dir.join("a.aiff");
    flac16(&f, 44_100, &src);
    aiff16(&a, 44_100, &src);
    let df = decode_file(&f, SampleRate(44_100), &CancelFlag::new()).unwrap();
    let da = decode_file(&a, SampleRate(44_100), &CancelFlag::new()).unwrap();
    assert_eq!(df.frames, src[0].len());
    assert_eq!(da.frames, src[0].len());
    // Lossless: both decode to the same quantised samples.
    assert_eq!(df.samples, da.samples);
    let first: Vec<f32> = df.view().channel(0).collect();
    let err: Vec<f32> = first.iter().zip(&src[0]).map(|(a, b)| a - b).collect();
    assert!(rms(&err) < 1e-4);
}

#[test]
fn missing_file_is_io_error() {
    let e = decode_file(
        std::path::Path::new("/nonexistent/x.wav"),
        R48,
        &CancelFlag::new(),
    );
    assert!(matches!(e, Err(MediaError::Io { .. })));
}

#[test]
fn cancelled_flag_aborts() {
    let dir = tmp("cancel");
    let p = dir.join("a.wav");
    wav16(&p, 44_100, &sine(44_100, 0.5, 440.0, 0.5, 2));
    let c = CancelFlag::new();
    c.cancel();
    assert!(matches!(
        decode_file(&p, R48, &c),
        Err(MediaError::Cancelled)
    ));
}

#[test]
fn truncated_and_garbage_files_never_panic() {
    let dir = tmp("fuzz");
    let src = sine(44_100, 1.0, 440.0, 0.5, 2);
    let w = dir.join("a.wav");
    let f = dir.join("a.flac");
    let a = dir.join("a.aiff");
    wav16(&w, 44_100, &src);
    flac16(&f, 44_100, &src);
    aiff16(&a, 44_100, &src);
    let mut seed = 12345u32;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed
    };
    for file in [&w, &f, &a] {
        let bytes = std::fs::read(file).unwrap();
        let ext = file.extension().unwrap().to_str().unwrap();
        for i in 0..40 {
            // Truncation at various points, including inside headers.
            let cut = (rnd() as usize) % bytes.len();
            let p = dir.join(format!("t{i}.{ext}"));
            std::fs::write(&p, &bytes[..cut]).unwrap();
            let _ = decode_file(&p, R48, &CancelFlag::new());
            // Random byte corruption.
            let mut b = bytes.clone();
            for _ in 0..8 {
                let at = (rnd() as usize) % b.len();
                b[at] = rnd() as u8;
            }
            let p = dir.join(format!("c{i}.{ext}"));
            std::fs::write(&p, &b).unwrap();
            let _ = decode_file(&p, R48, &CancelFlag::new());
        }
    }
    for (i, ext) in ["wav", "flac", "mp3", "m4a", "ogg", "aiff", "bin"]
        .iter()
        .enumerate()
    {
        let junk: Vec<u8> = (0..4096).map(|_| rnd() as u8).collect();
        let p = dir.join(format!("g{i}.{ext}"));
        std::fs::write(&p, junk).unwrap();
        assert!(decode_file(&p, R48, &CancelFlag::new()).is_err());
    }
    let empty = dir.join("empty.wav");
    std::fs::write(&empty, b"").unwrap();
    assert!(decode_file(&empty, R48, &CancelFlag::new()).is_err());
}

/// Release-mode timing: `cargo test -p opendeck-media --release -- --ignored --nocapture`.
#[test]
#[ignore = "timing; run in release"]
fn bench_five_minutes_44k_to_48k() {
    let dir = tmp("bench");
    let p = dir.join("long.wav");
    let secs = 300.0;
    let mut src = sine(44_100, secs, 220.0, 0.3, 2);
    let nz = noise(44_100, secs, 0.1, 2);
    for (c, n) in src.iter_mut().zip(&nz) {
        for (s, z) in c.iter_mut().zip(n) {
            *s += z;
        }
    }
    wav16(&p, 44_100, &src);
    let t = Instant::now();
    let d = decode_file(&p, R48, &CancelFlag::new()).unwrap();
    let el = t.elapsed();
    println!(
        "decode+resample 5 min 44.1k -> 48k: {el:?} ({} frames)",
        d.frames
    );
    assert!(el.as_secs_f64() < 2.0, "too slow: {el:?}");
}
