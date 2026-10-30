//! Accuracy against the labelled synthetic set. Prints the tables
//! (`cargo test -p opendeck-analysis --release --test accuracy -- --nocapture`).
#![allow(missing_docs)]

mod common;

use common::*;
use opendeck_analysis::{KeyAnalyzer, TempoAnalyzer};
use opendeck_contracts::analysis::{AnalysisResult, Analyzer, AudioView, CancelFlag};
use opendeck_contracts::key::KeyMode;
use opendeck_contracts::units::SampleRate;

fn run(a: &dyn Analyzer, mono: &[f64]) -> AnalysisResult {
    let ch = to_stereo(mono);
    let chans: [&[f32]; 2] = [&ch[0], &ch[1]];
    let mut out = AnalysisResult::default();
    a.analyze(
        AudioView {
            sample_rate: SampleRate(SR),
            channels: &chans,
        },
        &mut out,
        &CancelFlag::new(),
        &mut |_| {},
    )
    .expect("analyzes");
    out
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "about 90 s unoptimised; run with --release"
)]
fn tempo_accuracy() {
    let (mut bpm_ok, mut anchor_ok, mut n) = (0, 0, 0);
    for (case, seed) in tempo_set() {
        let r = run(&TempoAnalyzer::default(), &drums(case, seed));
        let bpm = r.bpm.map_or(0.0, |b| b.0);
        let anchor = r.beat_grid.map_or(-1.0, |g| g.anchor().0);
        let b_ok = (bpm - case.bpm).abs() <= 0.1;
        let a_ok = (anchor - case.offset).abs() <= 0.010;
        println!(
            "bpm {:6.2} off {:.3} swing {:.2} -> {:7.2} anchor {:.4} bpm_ok {b_ok} anchor_ok {a_ok}",
            case.bpm, case.offset, case.swing, bpm, anchor
        );
        n += 1;
        bpm_ok += i32::from(b_ok);
        anchor_ok += i32::from(a_ok);
    }
    println!("CUSTOM BPM {bpm_ok}/{n} anchor {anchor_ok}/{n}");
    assert!(f64::from(bpm_ok) >= 0.95 * f64::from(n), "bpm {bpm_ok}/{n}");
    assert!(
        f64::from(anchor_ok) >= 0.95 * f64::from(n),
        "anchor {anchor_ok}/{n}"
    );
}

#[test]
fn key_accuracy() {
    let (mut ok, mut exact, mut n) = (0, 0, 0);
    for (tonic, minor, cents) in key_set() {
        let r = run(&KeyAnalyzer, &pads(tonic, minor, cents, 24.0));
        let k = r.key.expect("key");
        let (t, m) = (k.tonic(), k.mode() == KeyMode::Minor);
        let is_exact = t == tonic && m == minor;
        let relative = if minor {
            !m && t == (tonic + 3) % 12
        } else {
            m && t == (tonic + 9) % 12
        };
        println!(
            "key {tonic:2} minor {minor:5} detune {cents:5.1} -> {} exact {is_exact} relative {relative}",
            k.musical()
        );
        n += 1;
        exact += i32::from(is_exact);
        ok += i32::from(is_exact || relative);
    }
    println!("CUSTOM KEY exact-or-relative {ok}/{n}, exact {exact}/{n}");
    assert!(f64::from(ok) >= 0.8 * f64::from(n), "key {ok}/{n}");
}
