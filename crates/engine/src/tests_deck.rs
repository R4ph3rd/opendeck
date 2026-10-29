//! Offline render tests for the M2 deck features: sync, loops, hot cues, beat
//! jump, slip and jog. Every render runs under `assert_no_alloc`.

use opendeck_contracts::cue::FrameRegion;
use opendeck_contracts::tempo::{SyncMode, TempoRange};
use opendeck_contracts::{Beats, EngineCommand, EngineEvent, Frames, Seconds};

use crate::tests_support::*;
use crate::{SyncStatus, TrackBuffer};

const BEAT_120: u64 = 24_000;

fn wrap_half(x: f64) -> f64 {
    (x + 0.5).rem_euclid(1.0) - 0.5
}

/// Phase difference of two playing decks in milliseconds, at the tempo of `a`.
fn phase_error_ms(rig: &Rig, a: (u8, f64), b: (u8, f64)) -> f64 {
    let pa = grid(a.1).beat_phase(Seconds(rig.pos(a.0) / f64::from(SR)));
    let pb = grid(b.1).beat_phase(Seconds(rig.pos(b.0) / f64::from(SR)));
    let bpm_a = a.1 * rig.status(a.0).rate;
    wrap_half(pb - pa) * 60.0 / bpm_a * 1000.0
}

fn events(rig: &mut Rig) -> Vec<EngineEvent> {
    let mut out = Vec::new();
    while let Some(e) = rig.r.handle().poll_event() {
        out.push(e);
    }
    out
}

fn sync(rig: &mut Rig, index: u8, mode: SyncMode) {
    rig.send(EngineCommand::SetSync {
        deck: deck(index),
        mode,
    });
}

// ----- Sync -----

#[test]
fn tempo_sync_matches_the_leaders_effective_bpm() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.load_click(1, 126.0, 60.0);
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(0)),
    });
    sync(&mut rig, 1, SyncMode::Tempo);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(4 * BLOCK);

    let t = rig.telemetry();
    let (b0, b1) = (
        t.decks[0].bpm.expect("grid").0,
        t.decks[1].bpm.expect("grid").0,
    );
    println!("synced: leader {b0:.6} BPM, follower {b1:.6} BPM");
    assert!(
        (b0 - 120.0).abs() < 1e-9 && (b1 - 120.0).abs() < 1e-6,
        "{b0} {b1}"
    );
    assert_eq!(t.leader, Some(deck(0)));
    assert_eq!(rig.status(1).sync, SyncStatus::Following);
    assert_eq!(rig.status(0).sync, SyncStatus::Off);

    // The follower tracks the leader's tempo fader.
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: 0.2,
    });
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    let want = 120.0 * TempoRange::Percent10.ratio(0.2);
    assert!((t.decks[0].bpm.expect("grid").0 - want).abs() < 1e-6);
    assert!((t.decks[1].bpm.expect("grid").0 - want).abs() < 1e-6);

    // Sync off: back to the follower's own fader.
    sync(&mut rig, 1, SyncMode::Off);
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    assert!((t.decks[1].bpm.expect("grid").0 - 126.0).abs() < 1e-6);
    assert_eq!(rig.status(1).sync, SyncStatus::Off);
}

#[test]
fn tempo_sync_uses_half_and_double_time() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 70.0, 30.0);
    rig.load_click(1, 140.0, 30.0);
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(0)),
    });
    sync(&mut rig, 1, SyncMode::Tempo);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    // 140 BPM at rate 1.0 is already double the leader: no change needed.
    assert!((t.decks[1].rate - 1.0).abs() < 1e-9, "{}", t.decks[1].rate);
}

#[test]
fn beat_sync_aligns_phase_within_a_millisecond() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.load_click(1, 126.0, 60.0);
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(0)),
    });
    rig.play(0, true);
    rig.step(SR as usize + 15_000);
    // Start the follower somewhere off the grid, beat sync already on.
    rig.seek(1, 7_777);
    sync(&mut rig, 1, SyncMode::Beat);
    rig.play(1, true);
    rig.step(BLOCK);

    let mut worst: f64 = 0.0;
    for _ in 0..20 {
        let error = phase_error_ms(&rig, (0, 120.0), (1, 126.0));
        worst = worst.max(error.abs());
        rig.step(SR as usize / 2 + 123);
    }
    println!("beat sync: worst phase error {worst:.4} ms over 10 s");
    assert!(worst < 1.0, "phase error {worst} ms");
    assert_eq!(rig.status(1).sync, SyncStatus::Following);
}

#[test]
fn beat_sync_pressed_on_a_playing_deck_aligns_once_and_holds() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.load_click(1, 126.0, 60.0);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(SR as usize);
    let before = phase_error_ms(&rig, (0, 120.0), (1, 126.0));
    sync(&mut rig, 1, SyncMode::Beat);
    rig.step(BLOCK);
    let after = phase_error_ms(&rig, (0, 120.0), (1, 126.0));
    println!("phase error before {before:.2} ms, after {after:.4} ms");
    assert!(after.abs() < 1.0, "{after}");
    // Nudge the follower off the grid with pitch bend: the offset is the
    // user's, it is not pulled back.
    rig.send(EngineCommand::PitchBend {
        deck: deck(1),
        amount: 1.0,
    });
    rig.step(SR as usize / 4);
    rig.send(EngineCommand::PitchBend {
        deck: deck(1),
        amount: 0.0,
    });
    rig.step(BLOCK);
    let nudged = phase_error_ms(&rig, (0, 120.0), (1, 126.0));
    assert!(
        nudged.abs() > 5.0,
        "bend should have moved the phase: {nudged}"
    );
    // Pressing beat sync again re-aligns.
    sync(&mut rig, 1, SyncMode::Beat);
    rig.step(BLOCK);
    assert!(phase_error_ms(&rig, (0, 120.0), (1, 126.0)).abs() < 1.0);
}

#[test]
fn beat_sync_survives_a_hot_cue_jump() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.load_click(1, 126.0, 60.0);
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(0)),
    });
    sync(&mut rig, 1, SyncMode::Beat);
    rig.quantize(1, false);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(SR as usize);
    rig.send(EngineCommand::SetHotCue {
        deck: deck(1),
        index: pad(0),
        cue: Some(FrameRegion {
            start: Frames(100_000),
            end: None,
        }),
    });
    rig.send(EngineCommand::HotCue {
        deck: deck(1),
        index: pad(0),
        pressed: true,
    });
    rig.step(3 * BLOCK);
    let error = phase_error_ms(&rig, (0, 120.0), (1, 126.0));
    assert!(error.abs() < 1.0, "{error}");
    assert!(
        rig.pos(1) > 85_000.0 && rig.pos(1) < 140_000.0,
        "{}",
        rig.pos(1)
    );
}

#[test]
fn leader_election_follows_the_playing_decks_with_four_decks() {
    let mut rig = Rig::new(4);
    for (i, bpm) in [120.0, 124.0, 128.0, 132.0].into_iter().enumerate() {
        rig.load_click(i as u8, bpm, 60.0);
    }
    for i in 0..4 {
        rig.play(i, true);
    }
    for i in 1..4 {
        sync(&mut rig, i, SyncMode::Tempo);
    }
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    assert_eq!(t.leader, Some(deck(0)), "first playing deck without sync");
    for i in 1..4 {
        let bpm = t.decks[i].bpm.expect("grid").0;
        assert!((bpm - 120.0).abs() < 1e-6, "deck {i}: {bpm}");
    }

    // An explicit leader wins; deck 0 is free (sync off), the others follow deck 2.
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(2),
        position: -0.5,
    });
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(2)),
    });
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    assert_eq!(t.leader, Some(deck(2)));
    assert_eq!(rig.status(2).sync, SyncStatus::Leader);
    let want = t.decks[2].bpm.expect("grid").0;
    for i in [1, 3] {
        assert!((t.decks[i].bpm.expect("grid").0 - want).abs() < 1e-6);
    }
    assert!((t.decks[0].bpm.expect("grid").0 - 120.0).abs() < 1e-9);

    // The leader is ejected: election picks another playing deck.
    rig.send(EngineCommand::Eject { deck: deck(2) });
    rig.step(4 * BLOCK);
    assert_eq!(rig.telemetry().leader, Some(deck(0)));

    // The new leader stops: the next playing deck takes over and keeps tempo.
    rig.play(0, false);
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    assert_eq!(t.leader, Some(deck(1)));
    let want = t.decks[1].bpm.expect("grid").0;
    assert!((t.decks[3].bpm.expect("grid").0 - want).abs() < 1e-6);
    assert_eq!(rig.status(3).sync, SyncStatus::Following);

    // Nobody left playing: no leader.
    rig.play(1, false);
    rig.play(3, false);
    rig.step(4 * BLOCK);
    assert_eq!(rig.telemetry().leader, None);
}

#[test]
fn a_leader_that_stops_hands_over_without_a_tempo_jump() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.load_click(1, 126.0, 60.0);
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: 0.3,
    });
    sync(&mut rig, 1, SyncMode::Tempo);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(4 * BLOCK);
    let want = 120.0 * TempoRange::Percent10.ratio(0.3);
    rig.play(0, false);
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    assert_eq!(t.leader, Some(deck(1)));
    assert!((t.decks[1].bpm.expect("grid").0 - want).abs() < 1e-6);
}

#[test]
fn without_a_grid_sync_does_nothing_and_says_so() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(1000.0, 10.0), None);
    rig.load_click(1, 126.0, 30.0);
    sync(&mut rig, 0, SyncMode::Beat);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(8 * BLOCK);
    let t = rig.telemetry();
    assert_eq!(rig.status(0).sync, SyncStatus::NoGrid);
    assert_eq!(t.decks[0].bpm, None);
    assert_eq!(t.decks[0].beat_phase, 0.0);
    assert!((t.decks[0].rate - 1.0).abs() < 1e-12);
    assert_ne!(t.leader, Some(deck(0)));
    // The grid deck is unaffected.
    assert!((t.decks[1].bpm.expect("grid").0 - 126.0).abs() < 1e-9);

    // A grid arriving later (analysis finished) enables sync.
    assert!(rig.r.handle().set_beat_grid(deck(0), Some(grid(120.0))));
    rig.step(8 * BLOCK);
    assert_eq!(rig.status(0).sync, SyncStatus::Following);
    assert!((rig.telemetry().decks[0].bpm.expect("grid").0 - 126.0).abs() < 1e-6);
}

#[test]
fn a_synced_deck_with_no_leader_reports_it() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 30.0);
    sync(&mut rig, 0, SyncMode::Tempo);
    rig.step(2 * BLOCK);
    assert_eq!(rig.status(0).sync, SyncStatus::NoLeader);
    rig.play(0, true);
    rig.step(2 * BLOCK);
    assert_eq!(rig.status(0).sync, SyncStatus::Leader);
}

// ----- Loops -----

/// 4 beats of a perfectly periodic click track: the loop repeats the source
/// exactly, so the output must match the track sample for sample.
#[test]
fn four_beat_loop_repeats_sample_exact() {
    let mut rig = Rig::new(2);
    let (track, g) = click_track(120.0, 60.0);
    let source = track.samples().to_vec();
    rig.load(0, track, Some(g));
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(4.0),
    });
    rig.play(0, true);
    let len = 4 * BEAT_120 as usize;
    let out = rig.step(5 * len);
    let events = events(&mut rig);
    assert!(
        events.contains(&EngineEvent::LoopSet {
            deck: deck(0),
            region: FrameRegion {
                start: Frames(0),
                end: Some(Frames(len as u64)),
            },
        }),
        "{events:?}"
    );
    let mut worst = 0.0_f32;
    for k in 1..5 {
        let chunk = &out[k * len * 2..(k + 1) * len * 2];
        for (a, b) in chunk.iter().zip(&source[..len * 2]) {
            worst = worst.max((a - b).abs());
        }
    }
    println!("loop repeats vs source: worst difference {worst:e}");
    assert!(worst < 1e-5, "{worst}");
    // The playhead is exactly where the wraps put it.
    let expected = (5 * len) % len;
    assert!(
        (rig.pos(0) - expected as f64).abs() < 1.0 + BLOCK as f64,
        "{}",
        rig.pos(0)
    );
    let status = rig.status(0);
    assert_eq!(status.active_loop, Some((0.0, len as f64)));
}

#[test]
fn loop_wrap_has_no_discontinuity_on_a_non_periodic_signal() {
    let mut rig = Rig::new(2);
    // 1000.3 Hz: the loop (2 s) ends 0.6 of a cycle off, the worst case for a click.
    let track = TrackBuffer::from_vec(sine_samples(1000.3, 30.0, 0.5)).expect("track");
    rig.load(0, track, Some(grid(120.0)));
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(4.0),
    });
    rig.play(0, true);
    rig.step(SR as usize / 2);
    let out = rig.step(8 * SR as usize);
    let step = max_step(&out);
    println!("loop wrap max sample step {step:.3} (sine alone 0.065)");
    assert!(step < 0.2, "click of {step}");
}

#[test]
fn loop_in_out_toggle_and_resize() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 20.0), None);
    rig.play(0, true);
    rig.step(10 * BLOCK);
    rig.send(EngineCommand::LoopIn { deck: deck(0) });
    rig.step(8 * BLOCK);
    let start = rig.pos(0);
    rig.send(EngineCommand::LoopOut { deck: deck(0) });
    rig.step(BLOCK);
    let region = rig.status(0).active_loop.expect("looping");
    assert!(region.0 < start + 1.0 && region.0 > start - 8.0 * BLOCK as f64 - 1.0);
    let len = region.1 - region.0;
    assert!((len - 8.0 * BLOCK as f64).abs() < 1.0, "loop length {len}");
    for _ in 0..40 {
        rig.step(BLOCK);
        let p = rig.pos(0);
        assert!(p >= region.0 - 1.0 && p < region.1 + BLOCK as f64, "{p}");
    }

    // Halve and double.
    rig.send(EngineCommand::LoopResize {
        deck: deck(0),
        factor: 0.5,
    });
    rig.step(BLOCK);
    let half = rig.status(0).active_loop.expect("looping");
    assert!((half.1 - half.0 - len / 2.0).abs() < 1.0);
    rig.send(EngineCommand::LoopResize {
        deck: deck(0),
        factor: 2.0,
    });
    rig.step(BLOCK);
    let back = rig.status(0).active_loop.expect("looping");
    assert!((back.1 - back.0 - len).abs() < 1.0);

    // Exit: the playhead leaves the loop. Toggle again: re-enter at the start.
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.step(40 * BLOCK);
    assert!(rig.status(0).active_loop.is_none());
    assert!(rig.pos(0) > region.1 + BLOCK as f64);
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.step(BLOCK);
    let again = rig.status(0).active_loop.expect("re-entered");
    assert_eq!(again, back);
    assert!(rig.pos(0) < again.0 + 2.0 * BLOCK as f64);

    // SetLoop recalls an explicit loop.
    rig.send(EngineCommand::SetLoop {
        deck: deck(0),
        region: FrameRegion {
            start: Frames(100_000),
            end: Some(Frames(110_000)),
        },
    });
    rig.step(BLOCK);
    assert_eq!(rig.status(0).active_loop, Some((100_000.0, 110_000.0)));
    rig.seek(0, 105_000);
    rig.step(80 * BLOCK);
    let p = rig.pos(0);
    assert!((100_000.0..110_000.0 + BLOCK as f64).contains(&p), "{p}");
}

#[test]
fn auto_loop_snaps_to_the_grid_only_with_quantize() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.quantize(0, true);
    rig.seek(0, 12_345);
    rig.step(BLOCK);
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(1.0),
    });
    rig.step(BLOCK);
    assert_eq!(
        rig.status(0).active_loop,
        Some((BEAT_120 as f64, 2.0 * BEAT_120 as f64)),
        "nearest beat to 12345 is beat 1"
    );
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.quantize(0, false);
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(1.0),
    });
    rig.step(BLOCK);
    let (start, end) = rig.status(0).active_loop.expect("loop");
    assert!((start - 12_345.0).abs() < 1.0 && (end - start - BEAT_120 as f64).abs() < 1.0);
}

#[test]
fn loops_without_a_grid_need_in_and_out() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 5.0), None);
    rig.play(0, true);
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(4.0),
    });
    rig.send(EngineCommand::LoopRoll {
        deck: deck(0),
        length: Beats(1.0),
        active: true,
    });
    rig.send(EngineCommand::BeatJump {
        deck: deck(0),
        beats: Beats(4.0),
    });
    rig.step(4 * BLOCK);
    assert!(rig.status(0).active_loop.is_none());
    assert!(
        (rig.pos(0) - 4.0 * BLOCK as f64).abs() < 1.0,
        "no grid, no jump"
    );
}

#[test]
fn loop_roll_returns_to_the_slip_position() {
    for quantize in [false, true] {
        let mut rig = Rig::new(2);
        let (track, g) = click_track(120.0, 60.0);
        rig.load(0, track, Some(g));
        rig.quantize(0, quantize);
        rig.play(0, true);
        rig.step(2 * BEAT_120 as usize);
        let start = rig.pos(0);
        rig.send(EngineCommand::LoopRoll {
            deck: deck(0),
            length: Beats(0.25),
            active: true,
        });
        let mut bursts = 0;
        let mut was_loud = false;
        for _ in 0..(BEAT_120 as usize / BLOCK) {
            let out = rig.step(BLOCK);
            let region = rig.status(0).active_loop.expect("rolling");
            let p = rig.pos(0);
            assert!(
                p >= region.0 - 1.0 && p <= region.1 + BLOCK as f64,
                "{p} in {region:?}"
            );
            let loud = peak(&out) > 0.2;
            if loud && !was_loud {
                bursts += 1;
            }
            was_loud = loud;
        }
        assert!(
            bursts >= 3,
            "the 1/4 beat roll repeats the click: {bursts} bursts"
        );
        let slip = rig.status(0).slip_position.expect("shadow playhead");
        assert!(slip > start + BEAT_120 as f64 * 0.9);

        rig.send(EngineCommand::LoopRoll {
            deck: deck(0),
            length: Beats(0.25),
            active: false,
        });
        rig.step(BLOCK);
        // Where playback would have been: nothing was lost to the roll.
        let elapsed = (BEAT_120 as usize / BLOCK) * BLOCK + BLOCK;
        let expected = start + elapsed as f64;
        let pos = rig.pos(0);
        println!("roll (quantize {quantize}): position {pos}, expected {expected}");
        assert!((pos - expected).abs() < 1.0, "{pos} vs {expected}");
        assert!(rig.status(0).active_loop.is_none());
        assert!(rig.status(0).slip_position.is_none());
    }
}

#[test]
fn loop_roll_restores_the_loop_that_was_active() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.play(0, true);
    rig.step(BLOCK);
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(4.0),
    });
    rig.step(BLOCK);
    let before = rig.status(0).active_loop;
    rig.send(EngineCommand::LoopRoll {
        deck: deck(0),
        length: Beats(0.5),
        active: true,
    });
    rig.step(4 * BLOCK);
    assert_ne!(rig.status(0).active_loop, before);
    rig.send(EngineCommand::LoopRoll {
        deck: deck(0),
        length: Beats(0.5),
        active: false,
    });
    rig.step(BLOCK);
    assert_eq!(rig.status(0).active_loop, before);
}

#[test]
fn key_locked_loops_stay_in_tune_and_in_the_loop() {
    let mut rig = Rig::new(2);
    let track = TrackBuffer::from_vec(sine_samples(1000.0, 30.0, 0.5)).expect("track");
    rig.load(0, track, Some(grid(120.0)));
    rig.send(EngineCommand::SetKeyLock {
        deck: deck(0),
        on: true,
    });
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: 0.8,
    });
    rig.send(EngineCommand::SetTempoRange {
        deck: deck(0),
        range: TempoRange::Percent10,
    });
    rig.play(0, true);
    rig.step(SR as usize);
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(1.0),
    });
    rig.step(SR as usize / 2);
    let region = rig.status(0).active_loop.expect("loop");
    let out = rig.step(3 * SR as usize);
    assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.0));
    let p = rig.pos(0);
    assert!(
        p >= region.0 - 1.0 && p <= region.1 + 2.0 * BLOCK as f64,
        "{p}"
    );
    // Key lock keeps 1 kHz at +8 %.
    let tail = &out[out.len() / 2..];
    let zero_crossings = left(tail)
        .windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count();
    let freq = zero_crossings as f64 / (tail.len() as f64 / 2.0 / f64::from(SR));
    assert!((freq - 1000.0).abs() < 60.0, "frequency {freq}");
    // Leaving the loop returns to straight playback.
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.step(SR as usize);
    assert!(rig.pos(0) > region.1);
}

// ----- Hot cues -----

#[test]
fn hot_cue_stores_previews_and_jumps() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 10.0), None);
    rig.seek(0, 12_000);
    rig.step(BLOCK);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: true,
    });
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: false,
    });
    rig.step(BLOCK);
    assert_eq!(
        events(&mut rig),
        vec![EngineEvent::HotCueSet {
            deck: deck(0),
            index: pad(2),
            cue: FrameRegion {
                start: Frames(12_000),
                end: None
            },
        }]
    );
    assert!(!rig.status(0).playing);

    // Stopped: preview while held, return on release.
    rig.seek(0, 0);
    rig.step(BLOCK);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: true,
    });
    let out = rig.step(4 * BLOCK);
    assert!(rig.status(0).playing);
    assert!(peak(&out) > 0.3);
    assert!(rig.pos(0) > 12_000.0);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: false,
    });
    rig.step(BLOCK);
    assert!(!rig.status(0).playing);
    assert_eq!(rig.pos(0), 12_000.0);

    // Playing: jump, and keep playing after release.
    rig.seek(0, 50_000);
    rig.play(0, true);
    rig.step(BLOCK);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: true,
    });
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: false,
    });
    rig.step(BLOCK);
    assert!(rig.status(0).playing);
    let p = rig.pos(0);
    assert!((p - (12_000.0 + BLOCK as f64)).abs() < 1.0, "{p}");

    // SetHotCue replaces and clears slots.
    rig.send(EngineCommand::SetHotCue {
        deck: deck(0),
        index: pad(2),
        cue: Some(FrameRegion {
            start: Frames(30_000),
            end: None,
        }),
    });
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: true,
    });
    rig.step(BLOCK);
    assert!((rig.pos(0) - 30_000.0 - BLOCK as f64).abs() < 1.0);
    rig.send(EngineCommand::SetHotCue {
        deck: deck(0),
        index: pad(2),
        cue: None,
    });
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(2),
        pressed: true,
    });
    rig.step(BLOCK);
    assert!(
        events(&mut rig)
            .iter()
            .any(|e| matches!(e, EngineEvent::HotCueSet { .. }))
    );
}

#[test]
fn hot_cue_is_quantized_to_the_next_beat() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 60.0);
    rig.play(0, true);
    rig.seek(0, 10_000);
    rig.step(BLOCK);
    let p = rig.pos(0);
    rig.send(EngineCommand::SetHotCue {
        deck: deck(0),
        index: pad(0),
        cue: Some(FrameRegion {
            start: Frames(2 * BEAT_120),
            end: None,
        }),
    });
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(0),
        pressed: true,
    });
    // Before the next beat (24000) nothing has moved.
    rig.step(BEAT_120 as usize - p as usize - 100);
    assert!(rig.pos(0) < BEAT_120 as f64, "{}", rig.pos(0));
    rig.step(1100);
    // Crossed beat 1 after 1000 frames: landed on the cue plus those frames.
    let expected = 2.0 * BEAT_120 as f64 + 1000.0;
    assert!(
        (rig.pos(0) - expected).abs() < 1.0,
        "{} vs {expected}",
        rig.pos(0)
    );
}

#[test]
fn hot_loop_starts_looping() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 10.0), None);
    rig.send(EngineCommand::SetHotCue {
        deck: deck(0),
        index: pad(1),
        cue: Some(FrameRegion {
            start: Frames(20_000),
            end: Some(Frames(30_000)),
        }),
    });
    rig.play(0, true);
    rig.step(BLOCK);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(1),
        pressed: true,
    });
    rig.step(40 * BLOCK);
    assert_eq!(rig.status(0).active_loop, Some((20_000.0, 30_000.0)));
    let p = rig.pos(0);
    assert!((20_000.0..30_000.0 + BLOCK as f64).contains(&p), "{p}");
}

#[test]
fn cue_snaps_to_the_grid_with_quantize() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 30.0);
    rig.seek(0, 13_000);
    rig.step(BLOCK);
    rig.send(EngineCommand::Cue {
        deck: deck(0),
        pressed: true,
    });
    rig.send(EngineCommand::Cue {
        deck: deck(0),
        pressed: false,
    });
    rig.step(BLOCK);
    assert_eq!(
        events(&mut rig),
        vec![EngineEvent::CuePointSet {
            deck: deck(0),
            position: Frames(BEAT_120),
        }]
    );
    assert_eq!(rig.pos(0), BEAT_120 as f64);
}

// ----- Beat jump -----

#[test]
fn beat_jump_keeps_the_beat_phase() {
    for fader in [0.0_f32, 0.8] {
        let mut rig = Rig::new(2);
        rig.load_click(0, 120.0, 120.0);
        rig.send(EngineCommand::SetTempoFader {
            deck: deck(0),
            position: fader,
        });
        rig.play(0, true);
        rig.step(30_000 + 123);
        let before = rig.pos(0);
        let rate = rig.status(0).rate;
        rig.send(EngineCommand::BeatJump {
            deck: deck(0),
            beats: Beats(8.0),
        });
        rig.step(BLOCK);
        let after = rig.pos(0);
        // The jump is 8 beats of source; the block after it plays on at `rate`.
        let jumped = after - before - BLOCK as f64 * rate;
        let g = grid(120.0);
        let phase = |p: f64| g.beat_phase(Seconds(p / f64::from(SR)));
        let gap = wrap_half(phase(after - BLOCK as f64 * rate) - phase(before));
        println!("fader {fader}: jumped {jumped:.1} frames, phase moved {gap:.2e} beats");
        assert!((jumped - 8.0 * BEAT_120 as f64).abs() < 1.0, "{jumped}");
        assert!(gap.abs() < 1e-6, "{gap}");
    }
    // Backwards, and a loop moves with the playhead.
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 120.0);
    rig.seek(0, 12 * BEAT_120);
    rig.step(BLOCK);
    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(4.0),
    });
    rig.step(BLOCK);
    let (start, end) = rig.status(0).active_loop.expect("loop");
    rig.send(EngineCommand::BeatJump {
        deck: deck(0),
        beats: Beats(-4.0),
    });
    rig.step(BLOCK);
    let (s2, e2) = rig.status(0).active_loop.expect("loop");
    assert_eq!(
        (s2, e2),
        (start - 4.0 * BEAT_120 as f64, end - 4.0 * BEAT_120 as f64)
    );
}

// ----- Slip -----

#[test]
fn slip_loop_returns_to_where_playback_would_be() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 30.0), None);
    rig.send(EngineCommand::SetSlip {
        deck: deck(0),
        on: true,
    });
    rig.play(0, true);
    rig.step(10 * BLOCK);
    rig.send(EngineCommand::LoopIn { deck: deck(0) });
    rig.step(8 * BLOCK);
    rig.send(EngineCommand::LoopOut { deck: deck(0) });
    rig.step(BLOCK);
    assert!(rig.telemetry().decks[0].slip_position.is_some());
    let shadow_at_out = rig.status(0).slip_position.expect("slip engaged");
    rig.step(2 * SR as usize);
    let shadow = rig.status(0).slip_position.expect("slip engaged");
    assert!((shadow - shadow_at_out - 2.0 * f64::from(SR)).abs() < 1.0);
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.step(BLOCK);
    let expected = shadow + BLOCK as f64;
    assert!(
        (rig.pos(0) - expected).abs() < 1.0,
        "{} vs {expected}",
        rig.pos(0)
    );
    assert!(rig.status(0).slip_position.is_none());
}

#[test]
fn slip_off_loop_exit_continues_from_the_loop() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 30.0), None);
    rig.play(0, true);
    rig.step(10 * BLOCK);
    rig.send(EngineCommand::LoopIn { deck: deck(0) });
    rig.step(8 * BLOCK);
    rig.send(EngineCommand::LoopOut { deck: deck(0) });
    rig.step(2 * SR as usize);
    assert!(rig.status(0).slip_position.is_none());
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.step(BLOCK);
    assert!(rig.pos(0) < 30_000.0, "stays near the loop: {}", rig.pos(0));
}

#[test]
fn slip_hot_cue_returns_on_release() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 30.0), None);
    rig.send(EngineCommand::SetSlip {
        deck: deck(0),
        on: true,
    });
    rig.send(EngineCommand::SetHotCue {
        deck: deck(0),
        index: pad(0),
        cue: Some(FrameRegion {
            start: Frames(200_000),
            end: None,
        }),
    });
    rig.play(0, true);
    rig.step(10 * BLOCK);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(0),
        pressed: true,
    });
    rig.step(20 * BLOCK);
    assert!(rig.pos(0) > 200_000.0);
    rig.send(EngineCommand::HotCue {
        deck: deck(0),
        index: pad(0),
        pressed: false,
    });
    rig.step(BLOCK);
    let expected = 10.0 * BLOCK as f64 + 20.0 * BLOCK as f64 + BLOCK as f64 + BLOCK as f64;
    assert!(
        (rig.pos(0) - expected).abs() < 2.0 * BLOCK as f64,
        "{}",
        rig.pos(0)
    );
}

// ----- Jog -----

fn jog(rig: &mut Rig, revolutions: f32) {
    rig.send(EngineCommand::JogTurn {
        deck: deck(0),
        revolutions,
    });
}

#[test]
fn touching_the_platter_in_vinyl_mode_holds_the_record() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 30.0), None);
    rig.play(0, true);
    rig.step(SR as usize / 2);
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: true,
    });
    rig.step(20 * BLOCK);
    let held = rig.pos(0);
    let out = rig.step(10 * BLOCK);
    assert!((rig.pos(0) - held).abs() < 1.0, "held record moved");
    assert!(peak(&out) < 0.01, "a held record is silent");
    assert!(rig.status(0).scratching);
    assert!(rig.status(0).playing, "transport keeps running underneath");

    // A quarter turn forward moves the audio by a quarter of 1.8 s.
    jog(&mut rig, 0.25);
    rig.step(SR as usize / 2);
    let moved = rig.pos(0) - held;
    let want = 0.25 * 1.8 * f64::from(SR);
    println!("quarter turn: moved {moved:.1} frames, want {want}");
    assert!((moved - want).abs() < 2.0, "{moved} vs {want}");

    // Backwards.
    jog(&mut rig, -0.25);
    rig.step(SR as usize / 2);
    assert!((rig.pos(0) - held).abs() < 4.0);

    // Release: back to tempo.
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: false,
    });
    rig.step(SR as usize / 2);
    assert!(!rig.status(0).scratching);
    let p0 = rig.pos(0);
    rig.step(BLOCK);
    assert!((rig.pos(0) - p0 - BLOCK as f64).abs() < 1.0);
    assert!((rig.status(0).rate - 1.0).abs() < 1e-6);
}

#[test]
fn scratching_a_stopped_deck_makes_sound_and_stops_with_the_hand() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(500.0, 30.0), None);
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: true,
    });
    rig.step(BLOCK);
    let mut loudest = 0.0_f32;
    for _ in 0..30 {
        jog(&mut rig, 0.01);
        let out = rig.step(BLOCK);
        loudest = loudest.max(peak(&out));
    }
    assert!(loudest > 0.2, "scrubbing is audible: {loudest}");
    let out = rig.step(30 * BLOCK);
    assert!(peak(&out[out.len() * 2 / 3..]) < 0.01);
    assert!(!rig.status(0).playing);
}

#[test]
fn scratch_speed_is_smooth_whatever_the_event_rate() {
    for every in [1usize, 2] {
        let mut rig = Rig::new(2);
        rig.load(0, sine(300.0, 60.0), None);
        rig.play(0, true);
        rig.step(BLOCK);
        rig.send(EngineCommand::JogTouch {
            deck: deck(0),
            touched: true,
        });
        let mut rates = Vec::new();
        for block in 0..80 {
            if block % every == 0 {
                // Constant wheel speed: 0.01 rev per block, delivered in bursts.
                jog(&mut rig, 0.01 * every as f32);
            }
            rig.step(BLOCK);
            rates.push(rig.status(0).rate);
        }
        let settled = &rates[30..];
        let mean = settled.iter().sum::<f64>() / settled.len() as f64;
        let swing = settled.iter().map(|r| (r - mean).abs()).fold(0.0, f64::max);
        let jump = settled
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f64::max);
        println!(
            "events every {every} block(s): mean speed {mean:.3}, swing {swing:.3}, largest block-to-block step {jump:.3}"
        );
        let want = 0.01 * 1.8 * f64::from(SR) / BLOCK as f64;
        assert!((mean - want).abs() < 0.05 * want, "{mean} vs {want}");
        assert!(
            swing < 0.15 * mean * every as f64 * every as f64,
            "swing {swing}"
        );
    }
}

#[test]
fn scratch_with_slip_returns_to_the_shadow_playhead() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(300.0, 60.0), None);
    rig.send(EngineCommand::SetSlip {
        deck: deck(0),
        on: true,
    });
    rig.play(0, true);
    rig.step(10 * BLOCK);
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: true,
    });
    for _ in 0..40 {
        jog(&mut rig, -0.02);
        rig.step(BLOCK);
    }
    assert!(rig.status(0).slip_position.is_some());
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: false,
    });
    rig.step(BLOCK);
    let expected = 10.0 * BLOCK as f64 + 41.0 * BLOCK as f64 + BLOCK as f64;
    assert!(
        (rig.pos(0) - expected).abs() < 2.0 * BLOCK as f64,
        "{}",
        rig.pos(0)
    );
    assert!(!rig.status(0).scratching);
}

#[test]
fn scratching_with_key_lock_on_still_works() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(300.0, 60.0), None);
    rig.send(EngineCommand::SetKeyLock {
        deck: deck(0),
        on: true,
    });
    rig.play(0, true);
    rig.step(SR as usize / 2);
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: true,
    });
    let held = {
        rig.step(10 * BLOCK);
        rig.pos(0)
    };
    jog(&mut rig, 0.2);
    let out = rig.step(SR as usize / 2);
    assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.0));
    assert!((rig.pos(0) - held - 0.2 * 1.8 * f64::from(SR)).abs() < 3.0);
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: false,
    });
    let out = rig.step(SR as usize);
    assert!(peak(&out[out.len() / 2..]) > 0.3, "key lock plays again");
}

#[test]
fn jog_nudge_is_a_temporary_rate_offset() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(300.0, 60.0), None);
    rig.send(EngineCommand::SetVinylMode {
        deck: deck(0),
        on: false,
    });
    rig.play(0, true);
    rig.step(BLOCK);
    // Touch means nothing without vinyl mode.
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: true,
    });
    for _ in 0..30 {
        jog(&mut rig, 0.005);
        rig.step(BLOCK);
    }
    assert!(!rig.status(0).scratching);
    let pushed = rig.status(0).rate;
    println!("nudge forward: rate {pushed:.4}");
    assert!(pushed > 1.01 && pushed <= 1.0 + crate::MAX_PITCH_BEND + 1e-9);
    for _ in 0..30 {
        jog(&mut rig, -0.005);
        rig.step(BLOCK);
    }
    assert!(rig.status(0).rate < 0.99);
    // Hands off: the rate returns to 1.0.
    rig.step(SR as usize / 2);
    assert!(
        (rig.status(0).rate - 1.0).abs() < 1e-6,
        "{}",
        rig.status(0).rate
    );
}

#[test]
fn jog_search_moves_by_beats() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 120.0);
    rig.send(EngineCommand::JogSearch {
        deck: deck(0),
        beats: Beats(4.0),
    });
    rig.step(BLOCK);
    assert_eq!(rig.pos(0), 4.0 * BEAT_120 as f64);
    rig.send(EngineCommand::JogSearch {
        deck: deck(0),
        beats: Beats(-1.5),
    });
    rig.step(BLOCK);
    assert_eq!(rig.pos(0), 2.5 * BEAT_120 as f64);
    // Without a grid a beat is half a second.
    rig.load(1, sine(500.0, 30.0), None);
    rig.send(EngineCommand::JogSearch {
        deck: deck(1),
        beats: Beats(2.0),
    });
    rig.step(BLOCK);
    assert_eq!(rig.pos(1), f64::from(SR));
}

#[test]
fn vinyl_mode_switch_while_touched_starts_and_stops_scratching() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(300.0, 30.0), None);
    rig.send(EngineCommand::SetVinylMode {
        deck: deck(0),
        on: false,
    });
    rig.play(0, true);
    rig.send(EngineCommand::JogTouch {
        deck: deck(0),
        touched: true,
    });
    rig.step(2 * BLOCK);
    assert!(!rig.status(0).scratching);
    rig.send(EngineCommand::SetVinylMode {
        deck: deck(0),
        on: true,
    });
    rig.step(2 * BLOCK);
    assert!(rig.status(0).scratching);
    rig.send(EngineCommand::SetVinylMode {
        deck: deck(0),
        on: false,
    });
    rig.step(SR as usize / 2);
    assert!(!rig.status(0).scratching);
}

// ----- Real-time safety -----

/// Everything the callback can be told, in random order, on four decks with
/// grids, under the allocation guard.
#[test]
fn every_m2_command_is_allocation_free() {
    let mut rig = Rig::new(4);
    for i in 0..4u8 {
        rig.load_click(i, 118.0 + 4.0 * f64::from(i), 20.0);
    }
    let mut seed = 0x9E37_79B9_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut buffer = vec![0.0; 777 * 2];
    let beats = [0.03125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0];
    for round in 0..3000u32 {
        let d = deck((next() % 4) as u8);
        let amount = (next() % 2001) as f32 / 1000.0 - 1.0;
        let flag = next() % 2 == 0;
        let b = Beats(beats[(next() % 7) as usize] * if next() % 5 == 0 { -1.0 } else { 1.0 });
        let command = match next() % 33 {
            0 => EngineCommand::SetPlaying {
                deck: d,
                playing: flag,
            },
            1 => EngineCommand::Cue {
                deck: d,
                pressed: flag,
            },
            2 => EngineCommand::Seek {
                deck: d,
                position: Frames(u64::from(next() % 900_000)),
            },
            3 => EngineCommand::SetSync {
                deck: d,
                mode: [SyncMode::Off, SyncMode::Tempo, SyncMode::Beat][(next() % 3) as usize],
            },
            4 => EngineCommand::SetLeader {
                deck: if flag { Some(d) } else { None },
            },
            5 => EngineCommand::JogTouch {
                deck: d,
                touched: flag,
            },
            6 => EngineCommand::JogTurn {
                deck: d,
                revolutions: amount * 0.05,
            },
            7 => EngineCommand::JogSearch { deck: d, beats: b },
            8 => EngineCommand::SetVinylMode { deck: d, on: flag },
            9 => EngineCommand::LoopIn { deck: d },
            10 => EngineCommand::LoopOut { deck: d },
            11 => EngineCommand::LoopToggle { deck: d },
            12 => EngineCommand::AutoLoop {
                deck: d,
                length: Beats(b.0.abs()),
            },
            13 => EngineCommand::LoopRoll {
                deck: d,
                length: Beats(b.0.abs()),
                active: flag,
            },
            14 => EngineCommand::LoopResize {
                deck: d,
                factor: if flag { 0.5 } else { 2.0 },
            },
            15 => EngineCommand::SetLoop {
                deck: d,
                region: FrameRegion {
                    start: Frames(u64::from(next() % 500_000)),
                    end: Some(Frames(u64::from(next() % 900_000))),
                },
            },
            16 => EngineCommand::BeatJump { deck: d, beats: b },
            17 => EngineCommand::SetHotCue {
                deck: d,
                index: pad((next() % 8) as u8),
                cue: flag.then_some(FrameRegion {
                    start: Frames(u64::from(next() % 900_000)),
                    end: None,
                }),
            },
            18 => EngineCommand::HotCue {
                deck: d,
                index: pad((next() % 8) as u8),
                pressed: flag,
            },
            19 => EngineCommand::SetSlip { deck: d, on: flag },
            20 => EngineCommand::SetQuantize { deck: d, on: flag },
            21 => EngineCommand::SetKeyLock { deck: d, on: flag },
            22 => EngineCommand::SetTempoFader {
                deck: d,
                position: amount,
            },
            23 => EngineCommand::PitchBend { deck: d, amount },
            24 => EngineCommand::SetEq {
                deck: d,
                band: [
                    opendeck_contracts::mixer::EqBand::Low,
                    opendeck_contracts::mixer::EqBand::Mid,
                    opendeck_contracts::mixer::EqBand::High,
                ][(next() % 3) as usize],
                gain: opendeck_contracts::Gain(amount.abs() * 2.0),
            },
            25 => EngineCommand::SetFilter { deck: d, amount },
            26 => EngineCommand::SetChannelFader {
                deck: d,
                position: amount.abs(),
            },
            27 => EngineCommand::SetPfl { deck: d, on: flag },
            28 => EngineCommand::SetCrossfader { position: amount },
            29 => EngineCommand::SetHeadphoneMix { mix: amount.abs() },
            30 => EngineCommand::SetSplitCue { on: flag },
            31 => EngineCommand::SetCrossfaderAssign {
                deck: d,
                assign: [
                    opendeck_contracts::mixer::CrossfaderAssign::A,
                    opendeck_contracts::mixer::CrossfaderAssign::Thru,
                    opendeck_contracts::mixer::CrossfaderAssign::B,
                ][(next() % 3) as usize],
            },
            _ => EngineCommand::SetTempoRange {
                deck: d,
                range: TempoRange::CYCLE[(next() % 4) as usize],
            },
        };
        let _ = rig.r.handle().send(command);
        if round % 250 == 0 {
            rig.r.handle().reclaim();
        }
        rig.r.render_into(&mut buffer);
        assert!(
            buffer.iter().all(|s| s.is_finite() && s.abs() < 12.0),
            "round {round}: {command:?}"
        );
        rig.r.handle().poll_telemetry();
        while rig.r.handle().poll_event().is_some() {}
    }
}

#[test]
fn telemetry_reports_sync_leader_and_loop_state() {
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 30.0);
    rig.load_click(1, 124.0, 30.0);
    sync(&mut rig, 1, SyncMode::Beat);
    rig.play(0, true);
    rig.play(1, true);
    rig.step(4 * BLOCK);

    let t = rig.telemetry();
    assert_eq!(t.leader, Some(deck(0)));
    assert!(t.decks[0].is_leader && !t.decks[1].is_leader);
    assert_eq!(t.decks[0].sync_mode, SyncMode::Off);
    assert_eq!(t.decks[1].sync_mode, SyncMode::Beat);
    let followed = t.decks[1].bpm.expect("grid").0;
    assert!(
        (followed - 120.0).abs() < 0.05,
        "follower at {followed} BPM"
    );
    assert!(t.decks[0].loop_region.is_none() && !t.decks[0].loop_active);

    rig.send(EngineCommand::AutoLoop {
        deck: deck(0),
        length: Beats(4.0),
    });
    rig.step(BLOCK);
    let t = rig.telemetry();
    let region = t.decks[0].loop_region.expect("loop set");
    assert!(t.decks[0].loop_active);
    assert_eq!(region.end.map(|e| e.0 - region.start.0), Some(4 * BEAT_120));

    // Exiting keeps the region for re-loop and clears the flag.
    rig.send(EngineCommand::LoopToggle { deck: deck(0) });
    rig.step(BLOCK);
    let t = rig.telemetry();
    assert!(!t.decks[0].loop_active);
    assert_eq!(t.decks[0].loop_region, Some(region));
}
