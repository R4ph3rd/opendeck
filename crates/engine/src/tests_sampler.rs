//! Offline render tests for the sampler. Every render goes through
//! `EngineCore::render`, which runs under `assert_no_alloc` in test builds.

use opendeck_contracts::sampler::SamplerMode;
use opendeck_contracts::{Beats, Bpm, EngineCommand, Gain, SamplerSlotId};

use crate::TrackBuffer;
use crate::tests_support::*;

fn slot(index: u8) -> SamplerSlotId {
    SamplerSlotId::new(index).expect("slot in range")
}

/// A sample that starts at full level: `0.9 * cos(2 pi 1 kHz t) * exp(-k / tau)`.
fn burst(seconds: f64, tau: f64) -> TrackBuffer {
    let frames = (seconds * f64::from(SR)) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for k in 0..frames {
        let t = k as f64 / f64::from(SR);
        let v = (0.9 * (2.0 * std::f64::consts::PI * 1000.0 * t).cos() * (-(k as f64) / tau).exp())
            as f32;
        out.push(v);
        out.push(v);
    }
    TrackBuffer::from_vec(out).expect("sample")
}

impl Rig {
    fn sample(&mut self, index: u8, track: TrackBuffer, bpm: Option<f64>) {
        self.r
            .handle()
            .load_sampler(slot(index), track, bpm.map(Bpm))
            .expect("hand-off ring has room");
        self.step(BLOCK);
    }

    fn press(&mut self, index: u8) {
        self.send(EngineCommand::SamplerTrigger {
            slot: slot(index),
            pressed: true,
        });
    }

    fn release(&mut self, index: u8) {
        self.send(EngineCommand::SamplerTrigger {
            slot: slot(index),
            pressed: false,
        });
    }

    fn mode(&mut self, index: u8, mode: SamplerMode) {
        self.send(EngineCommand::SetSamplerMode {
            slot: slot(index),
            mode,
        });
    }

    fn sampler_quantize(&mut self, on: bool) {
        self.send(EngineCommand::SetSamplerQuantize { on });
    }

    fn slot_status(&self, index: u8) -> crate::SamplerSlotStatus {
        self.r.core().sampler_status(slot(index))
    }

    /// Deck 0 plays a 120 BPM click track silently (channel fader down), as
    /// the master tempo reference. Returns after a few blocks.
    fn silent_master(&mut self, bpm: f64) {
        self.load_click(0, bpm, 60.0);
        self.send(EngineCommand::SetChannelFader {
            deck: deck(0),
            position: 0.0,
        });
        self.play(0, true);
        self.step(BLOCK * 4);
    }
}

fn first_above(left: &[f32], threshold: f32) -> Option<usize> {
    left.iter().position(|s| s.abs() > threshold)
}

/// Starts of bursts: the first sample above `threshold` after at least
/// `quiet` quiet frames.
fn onsets(left: &[f32], threshold: f32, quiet: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut last_loud: Option<usize> = None;
    for (i, s) in left.iter().enumerate() {
        if s.abs() > threshold {
            if last_loud.is_none_or(|l| i - l > quiet) {
                out.push(i);
            }
            last_loud = Some(i);
        }
    }
    out
}

#[test]
fn one_shot_plays_to_the_end_and_ignores_release() {
    let mut rig = Rig::new(2);
    rig.sample(0, sine(1000.0, 0.2), None);
    rig.press(0);
    let first = left(&rig.step(4800));
    rig.release(0);
    let second = left(&rig.step(4800 + 2000));
    assert!((rms(&first) - 0.35).abs() < 0.02, "{}", rms(&first));
    assert!((rms(&second[..4700]) - 0.35).abs() < 0.03);
    // 0.2 s = 9600 frames; the sample is over.
    assert_eq!(peak(&second[4800 + 100..]), 0.0);
    let s = rig.slot_status(0);
    assert!(s.loaded && !s.playing && !s.waiting);
}

#[test]
fn gate_plays_while_held_and_stops_on_release() {
    let mut rig = Rig::new(2);
    rig.sample(0, sine(1000.0, 2.0), None);
    rig.mode(0, SamplerMode::Gate);
    rig.press(0);
    let held = left(&rig.step(14_400));
    assert!((rms(&held[1000..]) - 0.35).abs() < 0.02);
    assert!(rig.slot_status(0).playing);
    rig.release(0);
    let after = rig.step(BLOCK);
    let l = left(&after);
    // 2 ms fade-out, then silence; no click at the cut.
    assert_eq!(peak(&l[200..]), 0.0);
    assert!(max_step(&after) < 0.12, "{}", max_step(&after));
    assert!(!rig.slot_status(0).playing);
}

#[test]
fn gate_tapped_inside_one_block_still_sounds() {
    let mut rig = Rig::new(2);
    rig.sample(0, burst(1.0, 20_000.0), None);
    rig.mode(0, SamplerMode::Gate);
    rig.press(0);
    rig.release(0);
    let out = left(&rig.step(BLOCK * 2));
    assert!(peak(&out[..BLOCK]) > 0.5, "the tap must be heard");
    assert_eq!(peak(&out[BLOCK + 200..]), 0.0);
}

#[test]
fn loop_repeats_without_a_click() {
    let mut rig = Rig::new(2);
    // 1 kHz for 0.1 s is 100 whole cycles: the loop seam is continuous.
    rig.sample(0, sine(1000.0, 0.1), None);
    rig.mode(0, SamplerMode::Loop);
    rig.press(0);
    let out = rig.step(SR as usize);
    let l = left(&out);
    assert!(max_step(&out) < 0.07, "step {}", max_step(&out));
    for window in l.chunks(4800).take(9) {
        assert!((rms(window) - 0.35).abs() < 0.02);
    }
    assert!(rig.slot_status(0).playing);
    rig.send(EngineCommand::SamplerStop { slot: slot(0) });
    let tail = left(&rig.step(BLOCK));
    assert_eq!(peak(&tail[200..]), 0.0);
    assert!(!rig.slot_status(0).playing);
}

#[test]
fn pressing_again_restarts_from_the_beginning() {
    let mut rig = Rig::new(2);
    rig.sample(0, burst(0.3, 480.0), None);
    rig.press(0);
    rig.step(4800);
    assert!(rig.slot_status(0).position >= 4799.0);
    rig.press(0);
    let out = rig.step(BLOCK);
    let l = left(&out);
    assert!(l[0].abs() > 0.8, "restart keeps its attack: {}", l[0]);
    assert!(rig.slot_status(0).position <= BLOCK as f64);
}

#[test]
fn stop_command_stops_a_playing_slot() {
    let mut rig = Rig::new(2);
    rig.sample(0, sine(1000.0, 2.0), None);
    rig.press(0);
    rig.step(2400);
    rig.send(EngineCommand::SamplerStop { slot: slot(0) });
    let out = rig.step(BLOCK);
    assert_eq!(peak(&left(&out)[200..]), 0.0);
    // Stopping an idle or empty slot is harmless.
    rig.send(EngineCommand::SamplerStop { slot: slot(5) });
    rig.press(5);
    assert_eq!(peak(&rig.step(BLOCK)), 0.0);
}

#[test]
fn slot_gain_and_bus_level_scale_the_output() {
    let mut rig = Rig::new(2);
    rig.sample(0, sine(1000.0, 4.0), None);
    rig.mode(0, SamplerMode::Loop);
    rig.press(0);
    rig.step(4800);
    let full = rms(&left(&rig.step(4800)));
    rig.send(EngineCommand::SetSamplerGain {
        slot: slot(0),
        gain: Gain(0.5),
    });
    rig.step(BLOCK);
    let half = rms(&left(&rig.step(4800)));
    rig.send(EngineCommand::SetSamplerLevel { gain: Gain(0.5) });
    rig.step(BLOCK);
    let quarter = rms(&left(&rig.step(4800)));
    assert!((half / full - 0.5).abs() < 0.02, "{}", half / full);
    assert!((quarter / full - 0.25).abs() < 0.02, "{}", quarter / full);
    rig.send(EngineCommand::SetSamplerLevel { gain: Gain(0.0) });
    rig.step(BLOCK);
    assert_eq!(peak(&rig.step(BLOCK)), 0.0);
}

#[test]
fn pfl_sends_the_sampler_to_the_headphones() {
    let mut rig = Rig::new(2);
    rig.sample(0, sine(1000.0, 4.0), None);
    rig.mode(0, SamplerMode::Loop);
    rig.press(0);
    let frames = 9600;
    let mut master = vec![0.0; frames * 2];
    let mut hp = vec![0.0; frames * 2];
    rig.r.render_both(&mut master, &mut hp);
    // Headphone mix defaults to cue only: no PFL, nothing to hear.
    assert!(rms(&left(&master)) > 0.3);
    assert!(peak(&hp) < 1e-6);
    rig.send(EngineCommand::SetSamplerPfl { on: true });
    rig.r.render_both(&mut master, &mut hp);
    rig.r.render_both(&mut master, &mut hp);
    assert!((rms(&left(&hp)) - 0.35).abs() < 0.02, "{}", rms(&left(&hp)));
    assert!(rms(&left(&master)) > 0.3);
}

#[test]
fn quantized_start_lands_on_the_next_beat() {
    for offset_blocks in [3, 7, 20] {
        let mut rig = Rig::new(2);
        rig.silent_master(120.0);
        rig.sample(0, burst(0.5, 4800.0), None);
        rig.step(BLOCK * offset_blocks);
        let pos = rig.pos(0);
        let beat = beat_frames(120.0);
        let expected = (pos / beat).ceil() * beat - pos;
        rig.press(0);
        let out = left(&rig.step(SR as usize));
        let first = first_above(&out, 0.05).expect("sample starts");
        assert!(
            (first as f64 - expected).abs() <= 2.0,
            "starts at {first}, beat in {expected}"
        );
        // Nothing earlier, and the sound is whole.
        assert!(out[first] > 0.8);
    }
}

#[test]
fn quantize_off_or_no_master_starts_at_once() {
    let mut rig = Rig::new(2);
    rig.sample(0, burst(0.5, 4800.0), None);
    rig.press(0);
    assert_eq!(first_above(&left(&rig.step(BLOCK)), 0.05), Some(0));

    let mut rig = Rig::new(2);
    rig.silent_master(120.0);
    rig.sample(0, burst(0.5, 4800.0), None);
    rig.step(BLOCK * 5);
    rig.sampler_quantize(false);
    rig.press(0);
    assert_eq!(first_above(&left(&rig.step(BLOCK)), 0.05), Some(0));
}

#[test]
fn gate_released_before_its_beat_never_starts() {
    let mut rig = Rig::new(2);
    rig.silent_master(120.0);
    rig.sample(0, burst(0.5, 4800.0), None);
    rig.mode(0, SamplerMode::Gate);
    // Make sure the next beat is a good way off.
    while rig.pos(0) % beat_frames(120.0) > 6000.0 || rig.pos(0) % beat_frames(120.0) < 100.0 {
        rig.step(BLOCK);
    }
    rig.press(0);
    rig.step(BLOCK);
    assert!(rig.slot_status(0).waiting);
    rig.release(0);
    let out = rig.step(SR as usize);
    assert_eq!(peak(&out), 0.0);
    assert!(!rig.slot_status(0).waiting);
}

/// Spacing of the bursts of a sample played through the sampler against the
/// 120 BPM master, in frames.
fn burst_spacing(sample_bpm: f64, sync: bool, tempo_fader: f32) -> (f64, f64) {
    let mut rig = Rig::new(2);
    rig.silent_master(120.0);
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: tempo_fader,
    });
    rig.step(SR as usize / 4);
    let (track, _) = click_track(sample_bpm, 12.0);
    rig.sample(0, track, Some(sample_bpm));
    rig.send(EngineCommand::SetSamplerSync {
        slot: slot(0),
        on: sync,
    });
    rig.sampler_quantize(false);
    rig.mode(0, SamplerMode::Loop);
    rig.press(0);
    let out = left(&rig.step(SR as usize * 6));
    let rate = rig.slot_status(0).rate;
    let marks = onsets(&out, 0.2, 2000);
    assert!(marks.len() >= 5, "{marks:?}");
    // Skip the first gap: the rate ramps in during the first block.
    let gaps: Vec<f64> = marks
        .windows(2)
        .skip(1)
        .map(|w| (w[1] - w[0]) as f64)
        .collect();
    (gaps.iter().sum::<f64>() / gaps.len() as f64, rate)
}

#[test]
fn sync_on_plays_at_the_master_tempo() {
    // 100 BPM sample against 120 BPM master: rate 1.2, one beat every 24000 frames.
    let (spacing, rate) = burst_spacing(100.0, true, 0.0);
    assert!((rate - 1.2).abs() < 1e-6, "{rate}");
    assert!((spacing - 24_000.0).abs() <= 3.0, "{spacing}");
    // Off: the sample's own tempo, 28800 frames per beat.
    let (spacing, rate) = burst_spacing(100.0, false, 0.0);
    assert_eq!(rate, 1.0);
    assert!((spacing - 28_800.0).abs() <= 3.0, "{spacing}");
}

#[test]
fn sync_follows_the_tempo_fader_and_folds_octaves() {
    let mut rig = Rig::new(2);
    rig.silent_master(120.0);
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: -1.0,
    });
    rig.step(SR as usize / 2);
    let deck_rate = rig.status(0).rate;
    assert!(deck_rate < 0.95);
    let master_bpm = 120.0 * deck_rate;
    let (track, _) = click_track(100.0, 12.0);
    rig.sample(0, track, Some(100.0));
    rig.send(EngineCommand::SetSamplerSync {
        slot: slot(0),
        on: true,
    });
    rig.sampler_quantize(false);
    rig.mode(0, SamplerMode::Loop);
    rig.press(0);
    let out = left(&rig.step(SR as usize * 6));
    let marks = onsets(&out, 0.2, 2000);
    let gaps: Vec<f64> = marks
        .windows(2)
        .skip(1)
        .map(|w| (w[1] - w[0]) as f64)
        .collect();
    let spacing = gaps.iter().sum::<f64>() / gaps.len() as f64;
    let expected = 28_800.0 / (master_bpm / 100.0);
    assert!(
        (spacing / expected - 1.0).abs() < 0.003,
        "{spacing} vs {expected}"
    );

    // A 60 BPM sample against 120 BPM folds to rate 1 (beats line up every other one).
    let (spacing, rate) = burst_spacing(60.0, true, 0.0);
    assert_eq!(rate, 1.0);
    assert!((spacing - 48_000.0).abs() <= 3.0, "{spacing}");
}

#[test]
fn sync_without_a_sample_bpm_or_master_plays_at_unity() {
    let mut rig = Rig::new(2);
    let (track, _) = click_track(100.0, 12.0);
    rig.sample(0, track, None);
    rig.send(EngineCommand::SetSamplerSync {
        slot: slot(0),
        on: true,
    });
    rig.silent_master(120.0);
    rig.press(0);
    rig.step(BLOCK * 2);
    assert_eq!(rig.slot_status(0).rate, 1.0);
    let (track, _) = click_track(100.0, 12.0);
    rig.sample(1, track, Some(100.0));
    rig.send(EngineCommand::SetSamplerSync {
        slot: slot(1),
        on: true,
    });
    rig.play(0, false);
    rig.step(BLOCK * 4);
    rig.press(1);
    rig.step(BLOCK * 2);
    assert_eq!(rig.slot_status(1).rate, 1.0, "no playing master");
}

#[test]
fn telemetry_reports_slots_and_the_bus_meter() {
    let mut rig = Rig::new(2);
    rig.sample(3, sine(1000.0, 1.0), None);
    rig.sample(9, sine(500.0, 1.0), None);
    rig.press(3);
    rig.step(SR as usize / 2);
    let t = rig.telemetry();
    assert!(t.sampler[3].loaded && t.sampler[3].playing);
    assert!(
        (t.sampler[3].progress - 0.5).abs() < 0.05,
        "{}",
        t.sampler[3].progress
    );
    assert!(t.sampler[9].loaded && !t.sampler[9].playing);
    assert_eq!(t.sampler[9].progress, 0.0);
    assert!(!t.sampler[0].loaded);
    assert!(t.sampler_meter.peak_l > 0.3);
}

#[test]
fn sample_swap_while_playing_returns_the_old_buffer() {
    let mut rig = Rig::new(2);
    for i in 0..16u8 {
        rig.sample(i, sine(200.0 + 50.0 * f64::from(i), 2.0), None);
        rig.mode(i, SamplerMode::Loop);
        rig.press(i);
    }
    rig.step(SR as usize / 4);
    assert_eq!(rig.r.handle().reclaim(), 0);
    for i in 0..16u8 {
        rig.sample(i, sine(300.0 + 50.0 * f64::from(i), 2.0), Some(120.0));
    }
    rig.step(BLOCK * 8);
    assert_eq!(rig.r.handle().reclaim(), 16, "every old sample comes back");
    // The swap cut the sounds; pressing again plays the new ones.
    assert_eq!(peak(&rig.step(BLOCK)), 0.0);
    rig.press(4);
    assert!(peak(&rig.step(BLOCK)) > 0.3);
    assert!(rig.r.handle().clear_sampler(slot(4)));
    rig.step(BLOCK * 2);
    assert_eq!(rig.r.handle().reclaim(), 1);
    assert!(!rig.slot_status(4).loaded);
    rig.press(4);
    assert_eq!(peak(&rig.step(BLOCK)), 0.0);
}

#[test]
fn loading_does_not_touch_gain_mode_or_sync() {
    let mut rig = Rig::new(2);
    rig.sample(0, sine(1000.0, 1.0), None);
    rig.send(EngineCommand::SetSamplerGain {
        slot: slot(0),
        gain: Gain(0.5),
    });
    rig.mode(0, SamplerMode::Loop);
    rig.sample(0, sine(1000.0, 0.05), None);
    rig.press(0);
    let out = left(&rig.step(SR as usize / 2));
    assert!(rig.slot_status(0).playing, "still a loop");
    assert!((rms(&out[2400..]) - 0.175).abs() < 0.015, "gain kept");
}

#[test]
fn sixteen_slots_fit_in_the_callback() {
    let mut rig = Rig::new(2);
    rig.silent_master(120.0);
    rig.send(EngineCommand::SetChannelFader {
        deck: deck(0),
        position: 1.0,
    });
    for i in 0..16u8 {
        let (track, _) = click_track(100.0 + f64::from(i), 8.0);
        rig.sample(i, track, Some(100.0 + f64::from(i)));
        rig.send(EngineCommand::SetSamplerSync {
            slot: slot(i),
            on: true,
        });
        rig.mode(i, SamplerMode::Loop);
        rig.press(i);
    }
    rig.step(SR as usize / 2);
    let before = rig.r.stats();
    let out = rig.r.render(SR as usize * 5);
    assert!(peak(&out) > 0.3);
    assert!((0..16u8).all(|i| rig.slot_status(i).playing));
    let mut s = rig.r.stats();
    s.blocks -= before.blocks;
    s.frames -= before.frames;
    s.total -= before.total;
    let fraction = s.realtime_fraction(SR);
    eprintln!(
        "sampler cpu: 16 synced loops + 1 deck: {:.2} % of realtime, worst block {:?}",
        fraction * 100.0,
        s.worst_block
    );
    if !cfg!(debug_assertions) {
        assert!(fraction < 0.25, "{fraction}");
    }
}

// ----- Beat loop pads -----

#[test]
fn auto_loop_covers_every_beat_pad_length() {
    // FLX4 BEAT LOOP pads: 1/4 to 32 beats (manual p.64); the engine takes
    // every standard size, 1/32 to 32.
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 90.0);
    rig.quantize(0, true);
    rig.play(0, true);
    rig.step(BLOCK * 4);
    let beat = beat_frames(120.0);
    for length in Beats::STANDARD_SIZES {
        rig.send(EngineCommand::AutoLoop {
            deck: deck(0),
            length,
        });
        rig.step(BLOCK);
        let (start, end) = rig.status(0).active_loop.expect("loop is active");
        let want = length.0 * beat;
        assert!(
            ((end - start) - want).abs() < 1.0,
            "{} beats: {} frames, want {want}",
            length.0,
            end - start
        );
        // Pressing the same pad again cancels the loop.
        rig.send(EngineCommand::LoopToggle { deck: deck(0) });
        rig.step(BLOCK);
        assert!(rig.status(0).active_loop.is_none());
        rig.step(SR as usize / 4);
    }
}
