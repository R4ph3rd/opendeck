//! Offline render tests for output routing (master, headphones, booth, mono)
//! and the input path (mic, aux, talk-over, ring behaviour). Every render
//! runs under `assert_no_alloc`; so do the input ring writes that stand in for
//! the input callback.

use opendeck_contracts::mixer::EqBand;
use opendeck_contracts::settings::{ChannelPair, InputChannels};
use opendeck_contracts::telemetry::EngineTelemetry;
use opendeck_contracts::{EngineCommand, Gain};

use crate::routing::{InputRouter, OutputRoute, Router, Routing};
use crate::tests_support::*;
use crate::{InputProducer, OfflineRenderer, TrackBuffer};

const PI: f64 = std::f64::consts::PI;

fn pair(left: u16, right: u16) -> ChannelPair {
    ChannelPair { left, right }
}

fn route(pair: ChannelPair, mono: bool) -> OutputRoute {
    OutputRoute { pair, mono }
}

/// Amplitude of the `freq` component of the left channel (whole cycles).
fn tone_amp(stereo: &[f32], freq: f64) -> f64 {
    let l = left(stereo);
    let n = l.len() as f64;
    let (mut s, mut c) = (0.0, 0.0);
    for (i, x) in l.iter().enumerate() {
        let phase = 2.0 * PI * freq * i as f64 / f64::from(SR);
        s += f64::from(*x) * phase.sin();
        c += f64::from(*x) * phase.cos();
    }
    2.0 / n * (s * s + c * c).sqrt()
}

fn send(r: &mut OfflineRenderer, command: EngineCommand) {
    r.handle().send(command).expect("command ring has room");
}

/// An engine with a synthetic input stream. `frame(i)` gives mic L, mic R,
/// aux L, aux R for the i-th frame since the start.
struct InputRig {
    r: OfflineRenderer,
    producer: InputProducer,
    next: usize,
}

impl InputRig {
    fn new() -> Self {
        let mut r = OfflineRenderer::new(config(2)).expect("engine");
        let producer = r.handle().take_input().expect("input writer");
        Self {
            r,
            producer,
            next: 0,
        }
    }

    /// Writes `frames` input frames, then renders as many output frames.
    fn run(&mut self, frames: usize, frame: impl Fn(usize) -> [f32; 4]) -> Vec<f32> {
        let mut out = Vec::with_capacity(frames * 2);
        for _ in 0..frames.div_ceil(BLOCK) {
            let n = BLOCK.min(frames - out.len() / 2);
            let mut data = Vec::with_capacity(n * 4);
            for i in 0..n {
                data.extend(frame(self.next + i));
            }
            self.next += n;
            let producer = &mut self.producer;
            let pushed = assert_no_alloc::assert_no_alloc(|| producer.push_frames(&data));
            assert_eq!(pushed, n, "ring has room");
            out.extend(self.r.render(n));
        }
        out
    }

    fn send(&mut self, command: EngineCommand) {
        send(&mut self.r, command);
    }

    fn telemetry(&mut self) -> EngineTelemetry {
        while self.r.handle().poll_telemetry().is_some() {}
        for _ in 0..8 {
            self.r.render(BLOCK);
            if let Some(t) = self.r.handle().poll_telemetry() {
                return t;
            }
        }
        panic!("no telemetry snapshot")
    }
}

fn sine_at(freq: f64, amp: f32, i: usize) -> f32 {
    amp * (2.0 * PI * freq * i as f64 / f64::from(SR)).sin() as f32
}

fn quiet_mic(_: usize) -> [f32; 4] {
    [0.0; 4]
}

fn mic_tone(freq: f64, amp: f32) -> impl Fn(usize) -> [f32; 4] {
    move |i| {
        let v = sine_at(freq, amp, i);
        [v, v, 0.0, 0.0]
    }
}

// ----- Output routing -----

/// Plays a 200 Hz tone on deck 0 with the cue bus on, fader down, so the
/// master is silent and the headphones carry the cue.
fn deck_tone(r: &mut OfflineRenderer) {
    let deck = deck(0);
    r.handle()
        .load_track(deck, sine(200.0, 6.0), None)
        .expect("room");
    send(
        r,
        EngineCommand::SetPlaying {
            deck,
            playing: true,
        },
    );
}

#[test]
fn booth_mirrors_the_master_with_its_own_level() {
    let mut r = OfflineRenderer::new(config(2)).expect("engine");
    deck_tone(&mut r);
    send(&mut r, EngineCommand::SetMasterLevel { gain: Gain(0.8) });
    let n = SR as usize;
    let (mut m, mut h, mut b) = (vec![0.0; n * 2], vec![0.0; n * 2], vec![0.0; n * 2]);
    r.render_all(&mut m, &mut h, &mut b);
    let (m, b) = (&m[n..], &b[n..]);
    assert!(rms(m) > 0.2, "master carries the tone");
    for (a, c) in m.iter().zip(b) {
        assert!((a - c).abs() < 1e-6, "unity booth is the master");
    }

    send(&mut r, EngineCommand::SetBoothLevel { gain: Gain(0.5) });
    let (mut m, mut h, mut b) = (vec![0.0; n * 2], vec![0.0; n * 2], vec![0.0; n * 2]);
    r.render_all(&mut m, &mut h, &mut b);
    let (m, b) = (&m[n..], &b[n..]);
    for (a, c) in m.iter().zip(b) {
        assert!((a * 0.5 - c).abs() < 1e-6, "booth is master x level");
    }
    // Master level moves the booth, booth level does not move the master.
    assert!((rms(m) - 0.8 * 0.5 / std::f64::consts::SQRT_2).abs() < 0.02);

    // Booth meter in telemetry.
    while r.handle().poll_telemetry().is_some() {}
    let mut buf = vec![0.0; BLOCK * 2];
    for _ in 0..8 {
        r.render_into(&mut buf);
    }
    let t = r.handle().poll_telemetry().expect("telemetry");
    assert!(t.booth.peak_l > 0.1 && t.booth.peak_l < t.master.peak_l);
}

#[test]
fn booth_level_is_clamped_and_ignores_nan() {
    let mut r = OfflineRenderer::new(config(2)).expect("engine");
    deck_tone(&mut r);
    send(
        &mut r,
        EngineCommand::SetBoothLevel {
            gain: Gain(f32::NAN),
        },
    );
    send(&mut r, EngineCommand::SetBoothLevel { gain: Gain(-3.0) });
    let n = SR as usize / 2;
    let (mut m, mut h, mut b) = (vec![0.0; n * 2], vec![0.0; n * 2], vec![0.0; n * 2]);
    r.render_all(&mut m, &mut h, &mut b);
    assert!(
        peak(&b[n..]) < 1e-6,
        "negative gain is silence, not inverted"
    );
    assert!(b.iter().all(|s| s.is_finite()));
}

/// Renders `frames` through `router` from a core whose deck 0 plays a 0.5 DC
/// track into the cue bus with the fader down.
fn render_router(router: &mut Router, channels: usize, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; channels * frames];
    let (mut core, mut handle) = crate::engine(config(2)).expect("engine");
    handle
        .load_track(
            deck(0),
            TrackBuffer::from_vec(vec![0.5; 96_000 * 2]).expect("track"),
            None,
        )
        .expect("room");
    for command in [
        EngineCommand::SetPfl {
            deck: deck(0),
            on: true,
        },
        EngineCommand::SetChannelFader {
            deck: deck(0),
            position: 0.0,
        },
        EngineCommand::SetHeadphoneLevel { gain: Gain(0.5) },
        EngineCommand::SetPlaying {
            deck: deck(0),
            playing: true,
        },
    ] {
        handle.send(command).expect("room");
    }
    router.render(&mut core, &mut out);
    out
}

#[test]
fn router_sends_the_cue_bus_to_the_headphone_pair() {
    let routing = Routing {
        master: route(pair(0, 1), false),
        headphones: Some(route(pair(2, 3), false)),
        booth: None,
    };
    let mut router = Router::new(4, routing, 64);
    let out = render_router(&mut router, 4, 4_000);
    for frame in out[4 * 3_000..].chunks_exact(4) {
        // Fader down: the master is silent, the cue is not.
        assert!(frame[0].abs() < 1e-6 && frame[1].abs() < 1e-6, "{frame:?}");
        assert!((frame[2] - 0.25).abs() < 1e-3, "{frame:?}");
        assert!((frame[3] - 0.25).abs() < 1e-3, "{frame:?}");
    }

    // No headphone pair: the cue goes nowhere, channels 3/4 stay silent.
    let routing = Routing {
        headphones: None,
        ..routing
    };
    let mut router = Router::new(4, routing, 64);
    let out = render_router(&mut router, 4, 4_000);
    assert!(out[4 * 3_000..].iter().all(|s| s.abs() < 1e-6));
}

#[test]
fn router_puts_master_headphones_and_booth_on_any_pairs_of_a_wide_device() {
    // 32 channel device, pairs at the top, in the middle and at the bottom.
    let routing = Routing {
        master: route(pair(30, 31), false),
        headphones: Some(route(pair(10, 11), false)),
        booth: Some(route(pair(0, 1), false)),
    };
    let mut router = Router::new(32, routing, 64);
    let mut out = vec![0.0_f32; 32 * 4_000];
    let (mut core, mut handle) = crate::engine(config(2)).expect("engine");
    handle
        .load_track(
            deck(0),
            TrackBuffer::from_vec(vec![0.5; 96_000 * 2]).expect("track"),
            None,
        )
        .expect("room");
    for command in [
        EngineCommand::SetPfl {
            deck: deck(0),
            on: true,
        },
        EngineCommand::SetBoothLevel { gain: Gain(0.5) },
        EngineCommand::SetPlaying {
            deck: deck(0),
            playing: true,
        },
    ] {
        handle.send(command).expect("room");
    }
    router.render(&mut core, &mut out);
    let frame = &out[32 * 3_500..32 * 3_501];
    assert!((frame[30] - 0.5).abs() < 1e-3 && (frame[31] - 0.5).abs() < 1e-3);
    assert!(
        (frame[0] - 0.25).abs() < 1e-3 && (frame[1] - 0.25).abs() < 1e-3,
        "booth"
    );
    assert!(frame[10].abs() > 0.1 && frame[11].abs() > 0.1, "headphones");
    for (i, s) in frame.iter().enumerate() {
        if ![0, 1, 10, 11, 30, 31].contains(&i) {
            assert_eq!(*s, 0.0, "channel {i} is silent");
        }
    }
}

#[test]
fn mono_outputs_play_the_sum_on_both_channels() {
    // Left channel only: stereo master puts it on the left, mono on both.
    let (mut core, mut handle) = crate::engine(config(2)).expect("engine");
    let mut samples = Vec::new();
    for _ in 0..96_000 {
        samples.extend([0.6_f32, 0.0]);
    }
    handle
        .load_track(
            deck(0),
            TrackBuffer::from_vec(samples).expect("track"),
            None,
        )
        .expect("room");
    handle
        .send(EngineCommand::SetPlaying {
            deck: deck(0),
            playing: true,
        })
        .expect("room");
    let routing = Routing {
        master: route(pair(0, 1), false),
        headphones: None,
        booth: Some(route(pair(2, 3), true)),
    };
    let mut router = Router::new(4, routing, 64);
    let mut out = vec![0.0_f32; 4 * 4_000];
    router.render(&mut core, &mut out);
    let frame = &out[4 * 3_500..4 * 3_501];
    assert!(
        (frame[0] - 0.6).abs() < 1e-3 && frame[1].abs() < 1e-3,
        "{frame:?}"
    );
    assert!(
        (frame[2] - 0.3).abs() < 1e-3 && (frame[3] - 0.3).abs() < 1e-3,
        "{frame:?}"
    );
}

#[test]
fn a_pair_outside_the_device_is_ignored_without_panicking() {
    let (mut core, mut handle) = crate::engine(config(2)).expect("engine");
    handle
        .load_track(deck(0), sine(200.0, 2.0), None)
        .expect("room");
    handle
        .send(EngineCommand::SetPlaying {
            deck: deck(0),
            playing: true,
        })
        .expect("room");
    let routing = Routing {
        master: route(pair(0, 1), false),
        headphones: Some(route(pair(6, 7), false)),
        booth: Some(route(pair(1, 9), false)),
    };
    let mut router = Router::new(2, routing, 64);
    let mut out = vec![0.0_f32; 2 * 2_000];
    router.render(&mut core, &mut out);
    assert!(out.iter().all(|s| s.is_finite()));
}

// ----- Inputs -----

#[test]
fn mic_reaches_the_master_and_the_meters() {
    let mut rig = InputRig::new();
    let out = rig.run(SR as usize / 2, mic_tone(1000.0, 0.25));
    let tail = &out[out.len() / 2..];
    assert!((tone_amp(tail, 1000.0) - 0.25).abs() < 0.01);
    let t = rig.telemetry();
    assert!(t.inputs.connected);
    assert!(t.inputs.mic.peak_l > 0.1 && t.inputs.mic.peak_l < 0.3);
    assert_eq!(t.inputs.aux.peak_l, 0.0);
    assert_eq!(t.inputs.duck_db, 0.0);
    assert!(t.master.peak_l > 0.0);
}

#[test]
fn mic_level_eq_and_switch() {
    let mut rig = InputRig::new();
    rig.send(EngineCommand::SetMicLevel { gain: Gain(0.5) });
    let out = rig.run(SR as usize, mic_tone(1000.0, 0.4));
    assert!((tone_amp(&out[out.len() / 2..], 1000.0) - 0.2).abs() < 0.01);

    // Low shelf kill: 60 Hz falls by more than 20 dB, 1 kHz stays put.
    let mut rig = InputRig::new();
    rig.send(EngineCommand::SetMicEq {
        band: EqBand::Low,
        gain: Gain(0.0),
    });
    let out = rig.run(SR as usize, |i| {
        let v = sine_at(60.0, 0.3, i) + sine_at(1000.0, 0.3, i);
        [v, v, 0.0, 0.0]
    });
    let tail = &out[out.len() / 2..];
    assert!(
        tone_amp(tail, 60.0) < 0.3 * 0.1,
        "low {}",
        tone_amp(tail, 60.0)
    );
    assert!((tone_amp(tail, 1000.0) - 0.3).abs() < 0.02);

    // High shelf kill: 12 kHz falls, 1 kHz stays put. `Mid` is ignored.
    let mut rig = InputRig::new();
    rig.send(EngineCommand::SetMicEq {
        band: EqBand::High,
        gain: Gain(0.0),
    });
    rig.send(EngineCommand::SetMicEq {
        band: EqBand::Mid,
        gain: Gain(0.0),
    });
    let out = rig.run(SR as usize, |i| {
        let v = sine_at(12_000.0, 0.3, i) + sine_at(1000.0, 0.3, i);
        [v, v, 0.0, 0.0]
    });
    let tail = &out[out.len() / 2..];
    assert!(tone_amp(tail, 12_000.0) < 0.3 * 0.1);
    assert!((tone_amp(tail, 1000.0) - 0.3).abs() < 0.02);

    // On/off: off fades to silence, on again brings it back.
    let mut rig = InputRig::new();
    rig.run(SR as usize / 4, mic_tone(1000.0, 0.3));
    rig.send(EngineCommand::SetMicOn { on: false });
    let out = rig.run(SR as usize / 2, mic_tone(1000.0, 0.3));
    assert!(peak(&out[out.len() / 2..]) < 1e-6, "mic off is silent");
    // The meter still shows the mic, so the level can be set before going live.
    let t = rig.telemetry();
    assert!(t.inputs.mic.peak_l > 0.1);
    rig.send(EngineCommand::SetMicOn { on: true });
    let out = rig.run(SR as usize / 2, mic_tone(1000.0, 0.3));
    assert!((tone_amp(&out[out.len() / 2..], 1000.0) - 0.3).abs() < 0.01);
}

#[test]
fn mono_mic_reaches_both_sides_through_the_router() {
    let (mut core, mut handle) = crate::engine(config(2)).expect("engine");
    let mut producer = handle.take_input().expect("writer");
    handle.set_input_prefill(64);
    // 4 channel interface, mic on channel 3 only.
    let router = InputRouter::new(4, Some(InputChannels::mono(3)), None);
    let data: Vec<f32> = (0..64).flat_map(|_| [0.9_f32, 0.9, 0.9, 0.2]).collect();
    let refused = assert_no_alloc::assert_no_alloc(|| router.process(&data, &mut producer));
    assert_eq!(refused, 0);
    let mut out = vec![0.0_f32; 64 * 2];
    core.render(&mut out);
    for frame in out.chunks_exact(2) {
        assert!((frame[0] - 0.2).abs() < 1e-6 && (frame[1] - 0.2).abs() < 1e-6);
    }
}

#[test]
fn aux_has_a_level_and_its_own_fader() {
    let aux = |i: usize| {
        let v = sine_at(500.0, 0.4, i);
        [0.0, 0.0, v, -v]
    };
    let mut rig = InputRig::new();
    let out = rig.run(SR as usize / 2, aux);
    assert!((tone_amp(&out[out.len() / 2..], 500.0) - 0.4).abs() < 0.01);
    // Stereo aux keeps its sides.
    let l = left(&out[out.len() / 2..]);
    let r: Vec<f32> = out[out.len() / 2..].chunks_exact(2).map(|f| f[1]).collect();
    assert!(l.iter().zip(&r).all(|(a, b)| (a + b).abs() < 1e-6));

    rig.send(EngineCommand::SetAuxFader { position: 0.5 });
    let out = rig.run(SR as usize / 2, aux);
    assert!((tone_amp(&out[out.len() / 2..], 500.0) - 0.2).abs() < 0.01);

    rig.send(EngineCommand::SetAuxLevel { gain: Gain(0.5) });
    let out = rig.run(SR as usize / 2, aux);
    assert!((tone_amp(&out[out.len() / 2..], 500.0) - 0.1).abs() < 0.01);

    rig.send(EngineCommand::SetAuxFader { position: 0.0 });
    let out = rig.run(SR as usize / 2, aux);
    assert!(peak(&out[out.len() / 2..]) < 1e-6, "fader down is silent");
    let t = rig.telemetry();
    assert!(t.inputs.aux.peak_l < 0.01, "aux meter is post fader");
}

#[test]
fn input_adds_to_the_program_and_follows_the_master_level() {
    let mut rig = InputRig::new();
    rig.r
        .handle()
        .load_track(deck(0), sine(200.0, 6.0), None)
        .expect("room");
    rig.send(EngineCommand::SetPlaying {
        deck: deck(0),
        playing: true,
    });
    rig.send(EngineCommand::SetMasterLevel { gain: Gain(0.5) });
    let out = rig.run(SR as usize, mic_tone(3000.0, 0.4));
    let tail = &out[out.len() / 2..];
    assert!(
        (tone_amp(tail, 200.0) - 0.25).abs() < 0.01,
        "program x master"
    );
    assert!((tone_amp(tail, 3000.0) - 0.2).abs() < 0.01, "mic x master");
}

#[test]
fn talk_over_ducks_the_program_while_the_mic_is_loud_and_recovers() {
    let mut rig = InputRig::new();
    rig.r
        .handle()
        .load_track(deck(0), sine(200.0, 30.0), None)
        .expect("room");
    rig.send(EngineCommand::SetPlaying {
        deck: deck(0),
        playing: true,
    });
    let out = rig.run(SR as usize / 2, quiet_mic);
    let reference = tone_amp(&out[out.len() / 2..], 200.0);
    assert!((reference - 0.5).abs() < 0.01);

    // Talk-over off: a loud mic does not duck.
    let out = rig.run(SR as usize / 2, mic_tone(3000.0, 0.3));
    assert!((tone_amp(&out[out.len() / 2..], 200.0) - reference).abs() < 0.01);

    rig.send(EngineCommand::SetTalkOver {
        on: true,
        depth_db: 12.0,
        threshold_db: -40.0,
    });
    let out = rig.run(SR as usize, mic_tone(3000.0, 0.3));
    let ducked = tone_amp(&out[out.len() / 2..], 200.0);
    let change = db(ducked / reference);
    assert!((change + 12.0).abs() < 0.5, "ducked by {change:.2} dB");
    // The mic itself is not ducked.
    assert!((tone_amp(&out[out.len() / 2..], 3000.0) - 0.3).abs() < 0.01);
    let t = rig.telemetry();
    assert!(
        (t.inputs.duck_db - 12.0).abs() < 0.6,
        "{}",
        t.inputs.duck_db
    );

    // Mic below the threshold: the program comes back (400 ms time constant).
    let out = rig.run(SR as usize * 4, mic_tone(3000.0, 0.001));
    let recovered = tone_amp(&out[out.len() - SR as usize / 2..], 200.0);
    assert!(
        db(recovered / reference).abs() < 0.3,
        "recovered {recovered}"
    );
    let t = rig.telemetry();
    assert!(t.inputs.duck_db < 0.05, "{}", t.inputs.duck_db);
}

#[test]
fn talk_over_needs_the_mic_on_and_clamps_its_settings() {
    let mut rig = InputRig::new();
    rig.r
        .handle()
        .load_track(deck(0), sine(200.0, 30.0), None)
        .expect("room");
    rig.send(EngineCommand::SetPlaying {
        deck: deck(0),
        playing: true,
    });
    rig.send(EngineCommand::SetTalkOver {
        on: true,
        depth_db: 12.0,
        threshold_db: -40.0,
    });
    rig.send(EngineCommand::SetMicOn { on: false });
    rig.run(SR as usize / 2, quiet_mic);
    let out = rig.run(SR as usize, mic_tone(3000.0, 0.3));
    assert!(
        (tone_amp(&out[out.len() / 2..], 200.0) - 0.5).abs() < 0.01,
        "a switched-off mic does not duck"
    );

    // Absurd values are clamped to 0..=40 dB, NaN is ignored.
    rig.send(EngineCommand::SetMicOn { on: true });
    rig.send(EngineCommand::SetTalkOver {
        on: true,
        depth_db: 500.0,
        threshold_db: f32::NAN,
    });
    let out = rig.run(SR as usize, mic_tone(3000.0, 0.3));
    let ducked = tone_amp(&out[out.len() / 2..], 200.0);
    assert!(
        (db(ducked / 0.5) + 40.0).abs() < 1.0,
        "{}",
        db(ducked / 0.5)
    );
}

#[test]
fn an_empty_ring_inserts_silence_counts_underruns_and_recovers() {
    let mut rig = InputRig::new();
    rig.run(BLOCK, mic_tone(1000.0, 0.3));
    // The writer stalls: the engine keeps rendering.
    let out = rig.r.render(BLOCK * 3);
    assert!(peak(&out[BLOCK * 2..]) < 1e-6, "silence while starved");
    let t = rig.telemetry();
    assert!(t.inputs.underruns >= 1, "underrun counted");
    assert!(t.inputs.connected);
    // Input resumes and is picked up again after it primes.
    let out = rig.run(BLOCK * 4, mic_tone(1000.0, 0.3));
    assert!(peak(&out[BLOCK * 4..]) > 0.2);
}

#[test]
fn a_writer_running_ahead_is_cut_back_and_counted() {
    let mut rig = InputRig::new();
    rig.r.handle().set_input_prefill(512);
    // 6000 frames queued at once: more than twice the cushion.
    let data = vec![0.1_f32; 6_000 * 4];
    let producer = &mut rig.producer;
    assert_eq!(producer.push_frames(&data), 6_000);
    rig.r.render(BLOCK);
    let t = rig.telemetry();
    assert!(t.inputs.overruns > 4_000, "cut {}", t.inputs.overruns);
    assert!(rig.producer.free_frames() > crate::INPUT_RING_FRAMES - 1_500);

    // A full ring refuses the rest and counts it.
    let big = vec![0.0_f32; (crate::INPUT_RING_FRAMES + 100) * 4];
    let accepted = rig.producer.push_frames(&big);
    assert!(accepted < crate::INPUT_RING_FRAMES + 100);
    let t = rig.telemetry();
    assert!(t.inputs.overruns > 4_000);
}

#[test]
fn without_an_input_stream_nothing_changes() {
    let mut rig = Rig::new(2);
    rig.r
        .handle()
        .load_track(deck(0), sine(200.0, 2.0), None)
        .expect("room");
    rig.play(0, true);
    let out = rig.step(SR as usize / 2);
    let t = rig.telemetry();
    assert!(!t.inputs.connected);
    assert_eq!(t.inputs.mic.peak_l, 0.0);
    assert!((tone_amp(&out[out.len() / 2..], 200.0) - 0.5).abs() < 0.01);
}

#[test]
fn a_closed_input_stream_reports_disconnected_and_ends_a_duck() {
    let mut rig = InputRig::new();
    rig.r
        .handle()
        .load_track(deck(0), sine(200.0, 30.0), None)
        .expect("room");
    rig.send(EngineCommand::SetPlaying {
        deck: deck(0),
        playing: true,
    });
    rig.send(EngineCommand::SetTalkOver {
        on: true,
        depth_db: 12.0,
        threshold_db: -40.0,
    });
    rig.run(SR as usize / 2, mic_tone(3000.0, 0.3));
    let InputRig {
        mut r, producer, ..
    } = rig;
    drop(producer);
    // The input vanishes mid-duck: the program is released, not stuck down.
    let out = r.render(SR as usize * 4);
    assert!(db(tone_amp(&out[out.len() - SR as usize..], 200.0) / 0.5).abs() < 0.3);
    for _ in 0..8 {
        r.render(BLOCK);
    }
    while r.handle().poll_telemetry().is_some() {}
    for _ in 0..8 {
        r.render(BLOCK);
    }
    let t = r.handle().poll_telemetry().expect("telemetry");
    assert!(!t.inputs.connected);
}

#[test]
fn every_input_command_survives_garbage_without_allocating() {
    let mut rig = InputRig::new();
    let values = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0, 1e9];
    for v in values {
        for command in [
            EngineCommand::SetMicLevel { gain: Gain(v) },
            EngineCommand::SetMicEq {
                band: EqBand::Low,
                gain: Gain(v),
            },
            EngineCommand::SetMicEq {
                band: EqBand::High,
                gain: Gain(v),
            },
            EngineCommand::SetAuxLevel { gain: Gain(v) },
            EngineCommand::SetAuxFader { position: v },
            EngineCommand::SetBoothLevel { gain: Gain(v) },
            EngineCommand::SetTalkOver {
                on: true,
                depth_db: v,
                threshold_db: v,
            },
            EngineCommand::SetMicOn { on: v > 0.0 },
        ] {
            rig.send(command);
        }
        let out = rig.run(BLOCK * 2, |i| {
            let s = sine_at(440.0, 1.0, i);
            [s, -s, s, s]
        });
        assert!(out.iter().all(|s| s.is_finite()), "finite after {v}");
    }
    let t = rig.telemetry();
    assert!(t.inputs.mic.peak_l.is_finite() && t.inputs.duck_db.is_finite());
}

#[test]
fn inputs_cost_little_at_the_target_block_size() {
    let mut rig = InputRig::new();
    rig.r
        .handle()
        .load_track(deck(0), sine(200.0, 30.0), None)
        .expect("room");
    rig.send(EngineCommand::SetPlaying {
        deck: deck(0),
        playing: true,
    });
    rig.send(EngineCommand::SetTalkOver {
        on: true,
        depth_db: 12.0,
        threshold_db: -40.0,
    });
    rig.send(EngineCommand::SetMicEq {
        band: EqBand::Low,
        gain: Gain(1.5),
    });
    rig.run(SR as usize * 2, |i| {
        let s = sine_at(440.0, 0.5, i);
        [s, s, s, -s]
    });
    let stats = rig.r.stats();
    println!(
        "mic + aux + talk-over: {:.2} % of realtime, worst block {:?}",
        stats.realtime_fraction(SR) * 100.0,
        stats.worst_block
    );
    if !cfg!(debug_assertions) {
        assert!(stats.realtime_fraction(SR) < 0.25);
    }
}

#[test]
fn test_tone_plays_on_one_bus_through_the_engine() {
    let mut r = OfflineRenderer::new(config(2)).expect("engine");
    for (output, carries) in [
        (crate::ToneOutput::Master, 0),
        (crate::ToneOutput::Headphones, 1),
        (crate::ToneOutput::Booth, 2),
    ] {
        r.handle().play_test_tone(output);
        let frames = SR as usize / 4;
        let mut buses = [
            vec![0.0; frames * 2],
            vec![0.0; frames * 2],
            vec![0.0; frames * 2],
        ];
        let [m, h, b] = &mut buses;
        r.render_all(m, h, b);
        for (index, bus) in buses.iter().enumerate() {
            let amp = tone_amp(bus, f64::from(crate::TEST_TONE_HZ));
            if index == carries {
                assert!(amp > 0.15, "{output:?} on bus {index}: {amp}");
            } else {
                assert!(amp < 1e-6, "{output:?} leaks to bus {index}: {amp}");
            }
        }
        // Let the tone end before the next one.
        r.render(SR as usize);
    }
}
