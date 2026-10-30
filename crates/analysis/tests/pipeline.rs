//! Behavioural tests of the full analyzer set: determinism, cancellation,
//! progress, degenerate input, range folding and speed.
#![allow(missing_docs)]

mod common;

use std::time::Instant;

use common::*;
use opendeck_analysis::{
    PEAK_LEVEL_FRAMES, TempoAnalyzer, TempoConfig, analyze_all, default_analyzers,
};
use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag,
};
use opendeck_contracts::units::SampleRate;

fn view<'a>(chans: &'a [&'a [f32]], rate: u32) -> AudioView<'a> {
    AudioView {
        sample_rate: SampleRate(rate),
        channels: chans,
    }
}

fn mix_track(seconds: f64) -> Vec<f64> {
    let d = drums(
        DrumCase {
            bpm: 126.0,
            offset: 0.2,
            swing: 0.1,
            noise_db: -40.0,
            jitter: 0.001,
            seconds,
        },
        7,
    );
    let p = pads(9, true, 0.0, seconds);
    d.iter().zip(&p).map(|(a, b)| 0.7 * a + 0.4 * b).collect()
}

#[test]
fn full_analysis_is_deterministic_and_complete() {
    let ch = to_stereo(&mix_track(30.0));
    let chans: [&[f32]; 2] = [&ch[0], &ch[1]];
    let run = || {
        let mut last = 0.0_f32;
        let mut monotonic = true;
        let r = analyze_all(
            view(&chans, SR),
            AnalysisFeatures::ALL,
            &CancelFlag::new(),
            &mut |p| {
                monotonic &= p >= last && (0.0..=1.0).contains(&p);
                last = p;
            },
        )
        .expect("analyzes");
        assert!(monotonic, "progress must be monotonic in [0, 1]");
        assert_eq!(last, 1.0);
        r
    };
    let a = run();
    let b = run();
    assert_eq!(a, b);
    assert!((a.bpm.expect("bpm").0 - 126.0).abs() <= 0.1);
    assert!((a.beat_grid.expect("grid").anchor().0 - 0.2).abs() <= 0.01);
    assert_eq!(a.key.expect("key").musical(), "Am");
    let l = a.loudness.expect("loudness");
    assert!(l.integrated_lufs < 0.0 && l.integrated_lufs > -40.0);
    assert_eq!(a.peaks.len(), PEAK_LEVEL_FRAMES.len() + 1);
}

#[test]
fn each_analyzer_writes_only_its_features() {
    let ch = to_stereo(&mix_track(12.0));
    let chans: [&[f32]; 2] = [&ch[0], &ch[1]];
    for a in default_analyzers() {
        let mut out = AnalysisResult::default();
        a.analyze(view(&chans, SR), &mut out, &CancelFlag::new(), &mut |_| {})
            .expect("analyzes");
        let f = a.features();
        assert_eq!(out.bpm.is_some() || out.beat_grid.is_some(), f.tempo);
        assert_eq!(out.key.is_some(), f.key);
        assert_eq!(out.loudness.is_some(), f.loudness);
        assert_eq!(!out.peaks.is_empty(), f.peaks);
    }
}

#[test]
fn cancel_stops_every_analyzer() {
    let ch = to_stereo(&mix_track(12.0));
    let chans: [&[f32]; 2] = [&ch[0], &ch[1]];
    let cancel = CancelFlag::new();
    cancel.cancel();
    for a in default_analyzers() {
        let mut out = AnalysisResult::default();
        assert_eq!(
            a.analyze(view(&chans, SR), &mut out, &cancel, &mut |_| {}),
            Err(AnalysisError::Cancelled),
            "{}",
            a.id()
        );
        assert_eq!(out, AnalysisResult::default(), "{}", a.id());
    }
}

#[test]
fn degenerate_input() {
    let short = vec![0.0_f32; 1000];
    let chans: [&[f32]; 1] = [&short];
    let mut out = AnalysisResult::default();
    for id in ["tempo", "key"] {
        let a = default_analyzers()
            .into_iter()
            .find(|a| a.id() == id)
            .expect("analyzer");
        assert!(matches!(
            a.analyze(view(&chans, SR), &mut out, &CancelFlag::new(), &mut |_| {}),
            Err(AnalysisError::InvalidInput(_))
        ));
    }
    // Silence: analyzes, finds nothing.
    let silence = vec![0.0_f32; SR as usize * 10];
    let chans: [&[f32]; 1] = [&silence];
    let r = analyze_all(
        view(&chans, SR),
        AnalysisFeatures::ALL,
        &CancelFlag::new(),
        &mut |_| {},
    )
    .expect("silence analyzes");
    assert!(r.bpm.is_none() && r.beat_grid.is_none() && r.key.is_none());
    assert!(r.loudness.is_some() && !r.peaks.is_empty());
}

#[test]
fn mono_and_48k_input() {
    // Generate at 44.1 kHz and relabel as 48 kHz: tempo scales by 48/44.1.
    let ch = to_stereo(&drums(
        DrumCase {
            bpm: 120.0,
            offset: 0.1,
            swing: 0.0,
            noise_db: -300.0,
            jitter: 0.0,
            seconds: 30.0,
        },
        3,
    ));
    let chans: [&[f32]; 1] = [&ch[0]];
    let mut out = AnalysisResult::default();
    TempoAnalyzer::default()
        .analyze(
            view(&chans, 48_000),
            &mut out,
            &CancelFlag::new(),
            &mut |_| {},
        )
        .expect("analyzes");
    let expect = 120.0 * 48_000.0 / 44_100.0;
    assert!(
        (out.bpm.expect("bpm").0 - expect).abs() <= 0.1,
        "{:?}",
        out.bpm
    );
}

#[test]
fn bpm_range_is_configurable() {
    let ch = to_stereo(&drums(
        DrumCase {
            bpm: 150.0,
            offset: 0.1,
            swing: 0.0,
            noise_db: -300.0,
            jitter: 0.0,
            seconds: 30.0,
        },
        5,
    ));
    let chans: [&[f32]; 2] = [&ch[0], &ch[1]];
    let analyze = |cfg: TempoConfig| {
        let mut out = AnalysisResult::default();
        TempoAnalyzer::new(cfg)
            .analyze(view(&chans, SR), &mut out, &CancelFlag::new(), &mut |_| {})
            .expect("analyzes");
        out.bpm.expect("bpm").0
    };
    assert!((analyze(TempoConfig::default()) - 150.0).abs() <= 0.1);
    let folded = analyze(TempoConfig {
        min_bpm: 60.0,
        max_bpm: 140.0,
    });
    assert!((folded - 75.0).abs() <= 0.1, "{folded}");
}

#[test]
fn five_minute_track_speed() {
    let seconds = if cfg!(debug_assertions) { 20.0 } else { 300.0 };
    let ch = to_stereo(&mix_track(seconds));
    let chans: [&[f32]; 2] = [&ch[0], &ch[1]];
    let t = Instant::now();
    let r = analyze_all(
        view(&chans, SR),
        AnalysisFeatures::ALL,
        &CancelFlag::new(),
        &mut |_| {},
    )
    .expect("analyzes");
    let took = t.elapsed();
    println!("analysis of {seconds} s of audio took {took:?}");
    for a in default_analyzers() {
        let mut out = AnalysisResult::default();
        let t = Instant::now();
        a.analyze(view(&chans, SR), &mut out, &CancelFlag::new(), &mut |_| {})
            .expect("analyzes");
        println!("  {:9} {:?}", a.id(), t.elapsed());
    }
    assert!((r.bpm.expect("bpm").0 - 126.0).abs() <= 0.1);
    if !cfg!(debug_assertions) {
        assert!(took.as_secs_f64() < 3.0, "too slow: {took:?}");
    }
}
