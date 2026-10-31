//! Key shift and keyboard mode through the stretcher, offline. The pitch is
//! measured from upward zero crossings of the left channel.

use opendeck_contracts::cue::{FrameRegion, HotCueIndex};
use opendeck_contracts::{EngineCommand, Frames};

use crate::TrackBuffer;
use crate::tests_support::*;

const A4: f64 = 440.0;

/// Frequency of `signal` from interpolated upward zero crossings, Hz.
fn frequency(signal: &[f32]) -> f64 {
    let mut first = None;
    let mut last = 0.0;
    let mut crossings = 0u32;
    for (i, w) in signal.windows(2).enumerate() {
        if w[0] <= 0.0 && w[1] > 0.0 {
            let t = i as f64 + f64::from(-w[0]) / f64::from(w[1] - w[0]);
            first.get_or_insert(t);
            last = t;
            crossings += 1;
        }
    }
    let first = first.expect("signal crosses zero");
    f64::from(crossings - 1) / ((last - first) / f64::from(SR))
}

fn shifted(freq: f64, semitones: i8) -> f64 {
    freq * 2.0_f64.powf(f64::from(semitones) / 12.0)
}

fn within(measured: f64, expected: f64, tolerance: f64) -> bool {
    (measured / expected - 1.0).abs() <= tolerance
}

fn key_shift(rig: &mut Rig, semitones: i8) {
    rig.send(EngineCommand::SetKeyShift {
        deck: deck(0),
        semitones,
    });
}

fn key_lock(rig: &mut Rig, on: bool) {
    rig.send(EngineCommand::SetKeyLock { deck: deck(0), on });
}

#[test]
fn key_shift_moves_pitch_by_the_semitones_with_key_lock_on_and_off() {
    for lock in [true, false] {
        for semitones in [-12, -7, -1, 1, 3, 7, 12] {
            let mut rig = Rig::new(1);
            rig.load(0, sine(A4, 6.0), None);
            key_lock(&mut rig, lock);
            key_shift(&mut rig, semitones);
            rig.play(0, true);
            rig.step(SR as usize / 2);
            let out = left(&rig.step(SR as usize));
            let f = frequency(&out);
            let want = shifted(A4, semitones);
            assert!(
                within(f, want, 0.01),
                "key lock {lock}, {semitones:+} st: {f:.2} Hz, want {want:.2}"
            );
            // Pitch moved, tempo did not.
            assert!((rig.status(0).rate - 1.0).abs() < 1e-9);
            assert!(rms(&out) > 0.25, "level holds: {}", rms(&out));
        }
    }
}

#[test]
fn key_shift_clamps_to_an_octave() {
    let mut rig = Rig::new(1);
    rig.load(0, sine(A4, 4.0), None);
    key_shift(&mut rig, 100);
    rig.play(0, true);
    rig.step(SR as usize / 2);
    let f = frequency(&left(&rig.step(SR as usize)));
    assert!(within(f, 2.0 * A4, 0.01), "{f}");
    key_shift(&mut rig, -128);
    rig.step(SR as usize / 2);
    let f = frequency(&left(&rig.step(SR as usize)));
    assert!(within(f, 0.5 * A4, 0.01), "{f}");
}

#[test]
fn key_shift_combines_with_tempo_in_both_key_lock_modes() {
    // Key lock on: tempo does not move the pitch, the shift does.
    let mut rig = Rig::new(1);
    rig.load(0, sine(A4, 8.0), None);
    key_lock(&mut rig, true);
    key_shift(&mut rig, 3);
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: 1.0,
    });
    rig.play(0, true);
    rig.step(SR as usize);
    let f = frequency(&left(&rig.step(SR as usize)));
    let rate = rig.status(0).rate;
    assert!(rate > 1.05);
    assert!(within(f, shifted(A4, 3), 0.01), "{f}");

    // Key lock off: pitch is the shift plus the tempo change.
    let mut rig = Rig::new(1);
    rig.load(0, sine(A4, 8.0), None);
    key_shift(&mut rig, 12);
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: 1.0,
    });
    rig.play(0, true);
    rig.step(SR as usize);
    let f = frequency(&left(&rig.step(SR as usize)));
    let rate = rig.status(0).rate;
    assert!(within(f, 2.0 * A4 * rate, 0.01), "{f} at rate {rate}");
}

#[test]
fn zero_shift_without_key_lock_is_plain_varispeed() {
    let play = |shift_then_back: bool| {
        let mut rig = Rig::new(1);
        rig.load(0, sine(A4, 3.0), None);
        if shift_then_back {
            key_shift(&mut rig, 5);
            key_shift(&mut rig, 0);
        }
        rig.play(0, true);
        rig.step(SR as usize)
    };
    assert_eq!(play(false), play(true));
}

#[test]
fn changing_the_shift_while_playing_is_continuous() {
    let mut rig = Rig::new(1);
    rig.load(0, sine(A4, 10.0), None);
    key_lock(&mut rig, true);
    rig.play(0, true);
    rig.step(SR as usize / 2);
    let mut all = Vec::new();
    for semitones in [5, -3, 12, 0, 7] {
        key_shift(&mut rig, semitones);
        all.extend(left(&rig.step(SR as usize)));
        let f = frequency(&all[all.len() - SR as usize / 2..]);
        assert!(
            within(f, shifted(A4, semitones), 0.01),
            "{semitones:+} st: {f}"
        );
    }
    // No dropouts or bursts anywhere in the sequence: 40 ms windows keep their level.
    for w in all.chunks(1920) {
        assert!(
            rms(w) > 0.1 && peak(w) < 0.9,
            "rms {} peak {}",
            rms(w),
            peak(w)
        );
    }
    // Leaving key shift with key lock on stays on the stretcher: still steady.
    assert!((rig.pos(0) - (SR as f64 * 5.5)).abs() < 2.0);
}

#[test]
fn leaving_the_shift_with_key_lock_off_returns_to_varispeed_without_a_click() {
    let mut rig = Rig::new(1);
    rig.load(0, sine(A4, 6.0), None);
    key_shift(&mut rig, 7);
    rig.play(0, true);
    rig.step(SR as usize);
    key_shift(&mut rig, 0);
    let out = rig.step(SR as usize);
    let l = left(&out);
    assert!(peak(&l) < 0.7, "{}", peak(&l));
    assert!(max_step(&out) < 0.2, "{}", max_step(&out));
    let f = frequency(&l[SR as usize / 2..]);
    assert!(within(f, A4, 0.01), "{f}");
}

/// A track with 220 Hz for two seconds, then 440 Hz.
fn two_part_track() -> TrackBuffer {
    let mut s = sine_samples(220.0, 2.0, 0.5);
    s.extend(sine_samples(A4, 4.0, 0.5));
    TrackBuffer::from_vec(s).expect("track")
}

const CUE: u64 = 2 * SR as u64;

fn keyboard_rig() -> Rig {
    let mut rig = Rig::new(1);
    rig.load(0, two_part_track(), None);
    rig.quantize(0, false);
    rig.send(EngineCommand::SetHotCue {
        deck: deck(0),
        index: HotCueIndex::new(0).expect("pad"),
        cue: Some(FrameRegion {
            start: Frames(CUE),
            end: None,
        }),
    });
    rig.step(BLOCK);
    rig
}

fn pad_edge(rig: &mut Rig, pressed: bool) {
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: HotCueIndex::new(0).expect("pad"),
        pressed,
    });
}

/// The app-core sequence for a keyboard pad on a stopped deck: key shift,
/// then the hot cue edge. Held, the deck previews the cue at the pitch;
/// released, it returns to the cue and stops.
#[test]
fn keyboard_pads_preview_the_hot_cue_at_each_pitch_while_stopped() {
    let mut rig = keyboard_rig();
    // FLX4 default pads (manual p.63): +4 .. +7 on the top row, 0 .. +3 below.
    for semitones in [0, 1, 4, 7, -5] {
        key_shift(&mut rig, semitones);
        pad_edge(&mut rig, true);
        rig.step(SR as usize / 4);
        let held = left(&rig.step(SR as usize / 2));
        let f = frequency(&held);
        assert!(
            within(f, shifted(A4, semitones), 0.01),
            "{semitones:+} st: {f:.2}"
        );
        assert!(rig.status(0).playing);
        assert!(rig.pos(0) > CUE as f64);

        pad_edge(&mut rig, false);
        rig.step(BLOCK * 2);
        assert!(!rig.status(0).playing, "released pad stops the deck");
        assert!((rig.pos(0) - CUE as f64).abs() < 1.0, "back on the cue");
        assert_eq!(peak(&rig.step(BLOCK)), 0.0);
    }
    // Leaving keyboard mode: app-core sends key shift 0.
    key_shift(&mut rig, 0);
    pad_edge(&mut rig, true);
    rig.step(SR as usize / 4);
    let f = frequency(&left(&rig.step(SR as usize / 2)));
    assert!(within(f, A4, 0.01), "{f}");
}

/// While the deck plays, the pad jumps to the cue at the new pitch (quantize
/// off here; with it on the jump waits for the next beat).
#[test]
fn keyboard_pad_jumps_a_playing_deck_to_the_cue_at_the_pitch() {
    let mut rig = keyboard_rig();
    rig.play(0, true);
    rig.step(SR as usize / 2);
    let before = frequency(&left(&rig.step(SR as usize / 4)));
    assert!(within(before, 220.0, 0.01), "{before}");

    key_shift(&mut rig, 4);
    pad_edge(&mut rig, true);
    rig.step(SR as usize / 4);
    let out = left(&rig.step(SR as usize / 2));
    let f = frequency(&out);
    assert!(within(f, shifted(A4, 4), 0.01), "{f}");
    assert!(rig.pos(0) > CUE as f64, "jumped into the 440 Hz part");
    pad_edge(&mut rig, false);
    rig.step(BLOCK);
    assert!(
        rig.status(0).playing,
        "release does not stop a playing deck"
    );
}
