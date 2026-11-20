//! Offline render tests for the preview voice. Every render goes through
//! `EngineCore::render`, which runs under `assert_no_alloc` in test builds.

use opendeck_contracts::preview::PreviewTelemetry;
use opendeck_contracts::{EngineCommand, Gain};

use crate::tests_support::*;
use crate::{PREVIEW_MASTER_GAIN, TrackBuffer};

/// A track whose left and right sample are `0.9 * frame / frames`, so the
/// output value says where the playhead is.
fn ramp(seconds: f64) -> TrackBuffer {
    let frames = (seconds * f64::from(SR)) as usize;
    let mut out = Vec::with_capacity(frames * 2);
    for k in 0..frames {
        let v = 0.9 * k as f32 / frames as f32;
        out.push(v);
        out.push(v);
    }
    TrackBuffer::from_vec(out).expect("track")
}

fn renders(rig: &mut Rig, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut master = vec![0.0; frames * 2];
    let mut phones = vec![0.0; frames * 2];
    rig.r.render_both(&mut master, &mut phones);
    (master, phones)
}

impl Rig {
    fn preview(&mut self, track: TrackBuffer, token: u32, fraction: f32) {
        self.r
            .handle()
            .load_preview(track, token, fraction)
            .expect("hand-off ring has room");
    }

    fn status_preview(&self) -> PreviewTelemetry {
        self.r.core().preview_status()
    }
}

#[test]
fn preview_goes_to_the_headphones_and_leaves_the_master_silent() {
    let mut rig = Rig::new(2);
    rig.preview(sine(1000.0, 4.0), 1, 0.0);
    renders(&mut rig, SR as usize / 10);
    let (master, phones) = renders(&mut rig, SR as usize / 4);
    assert!(peak(&master) < 1e-6, "master {}", peak(&master));
    let p = peak(&phones);
    println!("headphones peak {p:.3}");
    assert!((p - 0.5).abs() < 0.01, "{p}");
}

#[test]
fn preview_ignores_the_cue_master_blend_and_follows_the_headphone_level() {
    let mut rig = Rig::new(2);
    // Blend fully on master: a cued deck would vanish from the headphones.
    rig.send(EngineCommand::SetHeadphoneMix { mix: 1.0 });
    rig.preview(sine(1000.0, 4.0), 1, 0.0);
    renders(&mut rig, SR as usize / 10);
    let (_, full) = renders(&mut rig, SR as usize / 4);
    assert!((peak(&full) - 0.5).abs() < 0.01);

    rig.send(EngineCommand::SetHeadphoneLevel { gain: Gain(0.5) });
    renders(&mut rig, SR as usize / 10);
    let (_, half) = renders(&mut rig, SR as usize / 4);
    assert!((peak(&half) - 0.25).abs() < 0.01, "{}", peak(&half));
}

#[test]
fn without_a_headphone_pair_the_preview_joins_the_master_at_minus_12_db() {
    let mut rig = Rig::new(2);
    rig.send(EngineCommand::SetPreviewToMaster { on: true });
    rig.preview(sine(1000.0, 4.0), 1, 0.0);
    renders(&mut rig, SR as usize / 10);
    let (master, phones) = renders(&mut rig, SR as usize / 4);
    let m = peak(&master);
    println!(
        "master peak {m:.4}, expected {:.4}",
        0.5 * PREVIEW_MASTER_GAIN
    );
    assert!((f64::from(m) - 0.5 * f64::from(PREVIEW_MASTER_GAIN)).abs() < 0.005);
    assert!((db(f64::from(PREVIEW_MASTER_GAIN)) + 12.0).abs() < 0.01);
    assert!(peak(&phones) < 1e-6, "headphones stay silent");

    // The master level applies.
    rig.send(EngineCommand::SetMasterLevel { gain: Gain(0.5) });
    renders(&mut rig, SR as usize / 10);
    let (half, _) = renders(&mut rig, SR as usize / 4);
    assert!((f64::from(peak(&half)) - 0.5 * f64::from(m)).abs() < 0.005);
}

#[test]
fn preview_does_not_touch_the_decks_or_their_mix() {
    let render = |with_preview: bool| {
        let mut rig = Rig::new(2);
        rig.load(0, sine(440.0, 6.0), None);
        rig.play(0, true);
        if with_preview {
            rig.preview(sine(1000.0, 4.0), 1, 0.0);
        }
        let (master, _) = renders(&mut rig, SR as usize / 2);
        let status = rig.status(0);
        (master, status.playing, status.position)
    };
    let (alone, playing_a, pos_a) = render(false);
    let (with, playing_b, pos_b) = render(true);
    assert_eq!(
        alone, with,
        "the master is bit exact with a preview running"
    );
    assert_eq!((playing_a, playing_b), (true, true));
    assert_eq!(pos_a, pos_b);
}

#[test]
fn load_starts_at_the_fraction_and_play_jumps_there() {
    let mut rig = Rig::new(2);
    let frames = (SR as usize) * 10;
    rig.preview(ramp(10.0), 5, 0.5);
    renders(&mut rig, BLOCK);
    let t = rig.status_preview();
    assert!(t.loaded && t.playing);
    assert_eq!(t.token, 5);
    assert_eq!(t.duration.0, frames as u64);
    assert_eq!(t.position.0, (frames / 2 + BLOCK) as u64);

    // After the 2 ms fade-in the output is the ramp value at the playhead.
    let (_, phones) = renders(&mut rig, 256);
    let l = left(&phones);
    let expected = 0.9 * (frames / 2 + BLOCK + 255) as f32 / frames as f32;
    assert!((l[255] - expected).abs() < 1e-3, "{} vs {expected}", l[255]);

    rig.send(EngineCommand::PreviewPlay { fraction: 0.25 });
    renders(&mut rig, BLOCK);
    let t = rig.status_preview();
    assert_eq!(t.position.0, (frames / 4 + BLOCK) as u64);
    assert_eq!(t.token, 5, "same buffer, same token");
}

#[test]
fn out_of_range_and_nan_fractions_are_clamped() {
    let mut rig = Rig::new(2);
    rig.preview(ramp(2.0), 1, 0.0);
    renders(&mut rig, BLOCK);
    let end = 2 * u64::from(SR);
    for (fraction, at_least, at_most) in [
        (-3.0_f32, 0, 1),
        (f32::NAN, 0, 1),
        (f32::INFINITY, end - 1, end),
        (7.0, end - 1, end),
    ] {
        rig.send(EngineCommand::PreviewPlay { fraction });
        renders(&mut rig, 1);
        let p = rig.status_preview().position.0;
        assert!((at_least..=at_most).contains(&p), "{fraction}: {p}");
    }
}

#[test]
fn the_voice_stops_at_the_end_and_restarts_on_play() {
    let mut rig = Rig::new(2);
    rig.preview(sine(1000.0, 0.1), 3, 0.0);
    renders(&mut rig, SR as usize / 4);
    let t = rig.status_preview();
    assert!(t.loaded && !t.playing, "{t:?}");
    assert_eq!(t.position, t.duration);
    rig.send(EngineCommand::PreviewPlay { fraction: 0.0 });
    renders(&mut rig, BLOCK);
    assert!(rig.status_preview().playing);
}

#[test]
fn stop_fades_out_without_a_click() {
    let mut rig = Rig::new(2);
    rig.preview(sine(1000.0, 4.0), 1, 0.0);
    renders(&mut rig, SR as usize / 10);
    rig.send(EngineCommand::PreviewStop);
    let (_, phones) = renders(&mut rig, SR as usize / 10);
    // A 1 kHz tone at 0.5 steps by 0.065 per frame; a cut would jump by up to 0.5.
    assert!(max_step(&phones) < 0.1, "step {}", max_step(&phones));
    let tail = &phones[phones.len() - 200..];
    assert!(peak(tail) < 1e-6, "silent after the fade");
    let t = rig.status_preview();
    assert!(t.loaded && !t.playing);
}

#[test]
fn jumping_and_switching_buffers_do_not_click() {
    let mut rig = Rig::new(2);
    rig.preview(sine(1000.0, 4.0), 1, 0.0);
    renders(&mut rig, SR as usize / 10);
    let mut all = Vec::new();
    rig.send(EngineCommand::PreviewPlay { fraction: 0.37 });
    all.extend(renders(&mut rig, SR as usize / 20).1);
    rig.preview(sine(1000.0, 4.0), 2, 0.61);
    all.extend(renders(&mut rig, SR as usize / 20).1);
    // Two jumps between points of the same sine: the 2 ms fade keeps steps small.
    assert!(max_step(&all) < 0.15, "step {}", max_step(&all));
    assert_eq!(rig.status_preview().token, 2);
}

#[test]
fn replaced_buffers_come_back_for_freeing_off_the_audio_thread() {
    let mut rig = Rig::new(2);
    rig.preview(sine(1000.0, 1.0), 1, 0.0);
    renders(&mut rig, BLOCK);
    assert_eq!(rig.r.handle().reclaim(), 0);
    rig.preview(sine(500.0, 1.0), 2, 0.0);
    // The old buffer plays out its 2 ms fade, then leaves on the next block.
    renders(&mut rig, 2 * BLOCK);
    assert_eq!(rig.r.handle().reclaim(), 1, "the first buffer returned");
    rig.send(EngineCommand::PreviewStop);
    renders(&mut rig, BLOCK);
    assert_eq!(rig.r.handle().reclaim(), 0, "the loaded buffer stays");
}

#[test]
fn commands_without_a_buffer_do_nothing() {
    let mut rig = Rig::new(2);
    rig.send(EngineCommand::PreviewPlay { fraction: 0.5 });
    rig.send(EngineCommand::PreviewStop);
    let (master, phones) = renders(&mut rig, BLOCK);
    assert!(peak(&master) == 0.0 && peak(&phones) == 0.0);
    let t = rig.status_preview();
    assert!(!t.loaded && !t.playing);
    assert_eq!(rig.telemetry().preview, t);
}

#[test]
fn telemetry_reports_the_preview() {
    let mut rig = Rig::new(2);
    rig.preview(sine(1000.0, 2.0), 42, 0.0);
    let t = rig.telemetry().preview;
    assert!(t.loaded && t.playing);
    assert_eq!(t.token, 42);
    assert_eq!(t.duration.0, 2 * u64::from(SR));
    assert!(t.position.0 > 0);
}
