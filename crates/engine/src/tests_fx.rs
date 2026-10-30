//! Offline render tests for the FX host (milestone 5). Every render goes
//! through `EngineCore::render`, so the callback runs under `assert_no_alloc`
//! in debug builds. The effects here are the engine's own test effects; the
//! real ones live in `dsp-fx`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use opendeck_contracts::fx::{
    Effect, EffectInfo, EffectKey, FxRouting, ParamCurve, ParamDescriptor, ParamId, ProcessContext,
    StereoBufferMut,
};
use opendeck_contracts::ids::{FX_SEND_BUSES, FX_SLOTS_PER_UNIT};
use opendeck_contracts::{
    Beats, Bpm, EngineCommand, Frames, FxSlotId, FxUnit, Gain, MAX_DECKS, SampleRate,
};

use crate::tests_support::{BLOCK, Rig, SR, deck, left, max_step, peak, rms, sine};
use crate::{FxTempoSource, OfflineRenderer, SendTap, TrackBuffer};

// ----- Test effects -----

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    /// Multiplies by the `Amount` parameter (0..2, default 1).
    Gain,
    /// Feedback echo of 480 frames, 0.9 feedback: has a tail.
    Echo,
    /// Delays by `n` frames and reports it as latency.
    Latency(usize),
}

/// What the effect saw, readable from the test thread.
#[derive(Default)]
pub(crate) struct Probe {
    last_param: AtomicU32,
    param_calls: AtomicU32,
    bpm: AtomicU64,
    beat: AtomicU64,
    division: AtomicU64,
    playing: AtomicBool,
    blocks: AtomicU32,
    prepared: AtomicU32,
    resets: AtomicU32,
    dropped: AtomicBool,
}

struct TestFx {
    info: EffectInfo,
    kind: Kind,
    gain: f32,
    probe: Arc<Probe>,
    line_l: Vec<f32>,
    line_r: Vec<f32>,
    pos: usize,
}

fn info(kind: Kind) -> EffectInfo {
    EffectInfo {
        key: EffectKey {
            provider: "test".into(),
            id: "fx".into(),
        },
        name: "Test".into(),
        routing: FxRouting::Insert,
        beat_synced: true,
        has_tail: matches!(kind, Kind::Echo),
        params: vec![ParamDescriptor {
            id: ParamId(0),
            name: "Amount".into(),
            unit: opendeck_contracts::fx::ParamUnit::None,
            min: 0.0,
            max: 2.0,
            default: 1.0,
            curve: ParamCurve::Linear,
        }],
    }
}

pub(crate) fn fx(kind: Kind) -> (Box<dyn Effect>, Arc<Probe>) {
    let probe = Arc::new(Probe::default());
    let effect = TestFx {
        info: info(kind),
        kind,
        gain: 1.0,
        probe: Arc::clone(&probe),
        line_l: Vec::new(),
        line_r: Vec::new(),
        pos: 0,
    };
    (Box::new(effect), probe)
}

pub(crate) fn gain_fx(amount: f32) -> (Box<dyn Effect>, Arc<Probe>) {
    let (mut e, p) = fx(Kind::Gain);
    e.set_parameter(ParamId(0), amount / 2.0);
    p.param_calls.store(0, Ordering::Relaxed);
    (e, p)
}

impl Drop for TestFx {
    fn drop(&mut self) {
        self.probe.dropped.store(true, Ordering::Relaxed);
    }
}

impl Effect for TestFx {
    fn info(&self) -> &EffectInfo {
        &self.info
    }

    fn prepare(&mut self, _: SampleRate, _: usize) {
        let n = match self.kind {
            Kind::Gain => 0,
            Kind::Echo => 480,
            Kind::Latency(n) => n,
        };
        self.line_l = vec![0.0; n];
        self.line_r = vec![0.0; n];
        self.pos = 0;
        self.probe.prepared.fetch_add(1, Ordering::Relaxed);
    }

    fn reset(&mut self) {
        self.line_l.fill(0.0);
        self.line_r.fill(0.0);
        self.pos = 0;
        self.probe.resets.fetch_add(1, Ordering::Relaxed);
    }

    fn set_parameter(&mut self, id: ParamId, normalized: f32) {
        if id == ParamId(0) {
            self.gain = self.info.params[0].to_plain(normalized);
            self.probe
                .last_param
                .store(normalized.to_bits(), Ordering::Relaxed);
            self.probe.param_calls.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn process(&mut self, mut io: StereoBufferMut<'_>, ctx: &ProcessContext) {
        let p = &self.probe;
        p.bpm
            .store(ctx.bpm.map_or(0.0, |b| b.0).to_bits(), Ordering::Relaxed);
        p.beat.store(
            ctx.beat_position.map_or(-1.0, |b| b.0).to_bits(),
            Ordering::Relaxed,
        );
        p.division
            .store(ctx.beat_division.0.to_bits(), Ordering::Relaxed);
        p.playing.store(ctx.playing, Ordering::Relaxed);
        p.blocks.fetch_add(1, Ordering::Relaxed);
        let (l, r) = io.channels();
        match self.kind {
            Kind::Gain => {
                for s in l.iter_mut().chain(r.iter_mut()) {
                    *s *= self.gain;
                }
            }
            Kind::Echo => {
                let n = self.line_l.len();
                for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                    let (yl, yr) = (
                        *a + 0.9 * self.line_l[self.pos],
                        *b + 0.9 * self.line_r[self.pos],
                    );
                    self.line_l[self.pos] = yl;
                    self.line_r[self.pos] = yr;
                    self.pos = (self.pos + 1) % n;
                    *a = yl;
                    *b = yr;
                }
            }
            Kind::Latency(n) => {
                for (a, b) in l.iter_mut().zip(r.iter_mut()) {
                    let (yl, yr) = (self.line_l[self.pos], self.line_r[self.pos]);
                    self.line_l[self.pos] = *a;
                    self.line_r[self.pos] = *b;
                    self.pos = (self.pos + 1) % n;
                    *a = yl;
                    *b = yr;
                }
            }
        }
    }

    fn latency_frames(&self) -> u32 {
        match self.kind {
            Kind::Latency(n) => n as u32,
            _ => 0,
        }
    }
}

fn f64_of(a: &AtomicU64) -> f64 {
    f64::from_bits(a.load(Ordering::Relaxed))
}

// ----- Helpers -----

fn slot(unit: FxUnit, index: usize) -> FxSlotId {
    FxSlotId::new(unit, index as u8).expect("slot in range")
}

fn install(rig: &mut Rig, id: FxSlotId, effect: Box<dyn Effect>) {
    rig.r
        .handle()
        .install_effect(id, effect)
        .map_err(|_| "ring full")
        .expect("hand-off ring has room");
}

fn enable(rig: &mut Rig, id: FxSlotId, on: bool) {
    rig.send(EngineCommand::SetFxEnabled { slot: id, on });
}

fn mix(rig: &mut Rig, id: FxSlotId, wet: f32) {
    rig.send(EngineCommand::SetFxMix { slot: id, wet });
}

fn status(rig: &Rig, id: FxSlotId) -> crate::FxSlotStatus {
    rig.r.core().fx_slot_status(id)
}

/// Two decks with different sines. `play` picks which run. `setup` installs
/// effects and sends commands before play. Returns the last `keep` frames of
/// master after `total` frames.
fn run(play: [bool; 2], total: usize, keep: usize, setup: impl FnOnce(&mut Rig)) -> Vec<f32> {
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 4.0), None);
    rig.load(1, sine(1000.0, 4.0), None);
    setup(&mut rig);
    for (i, on) in play.into_iter().enumerate() {
        if on {
            rig.play(i as u8, true);
        }
    }
    let out = rig.step(total);
    out[(total - keep) * 2..].to_vec()
}

fn assert_close(a: &[f32], b: &[f32], tol: f32, what: &str) {
    assert_eq!(a.len(), b.len());
    let worst = a
        .iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max);
    assert!(worst < tol, "{what}: worst difference {worst}");
}

const TOTAL: usize = 24_000;
const KEEP: usize = 12_000;

// ----- Insert chains -----

#[test]
fn insert_chain_on_deck_a_changes_only_deck_a() {
    let a = run([true, false], TOTAL, KEEP, |_| {});
    let b = run([false, true], TOTAL, KEEP, |_| {});
    let both = run([true, true], TOTAL, KEEP, |_| {});
    let summed: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
    assert_close(&both, &summed, 1e-5, "reference sum");

    let id = slot(FxUnit::Deck(deck(0)), 0);
    let with_fx = run([true, true], TOTAL, KEEP, |rig| {
        let (e, _) = gain_fx(0.5);
        install(rig, id, e);
        mix(rig, id, 1.0);
        enable(rig, id, true);
    });
    let expected: Vec<f32> = a.iter().zip(&b).map(|(x, y)| 0.5 * x + y).collect();
    assert_close(&with_fx, &expected, 1e-4, "deck A halved, deck B untouched");
}

#[test]
fn master_unit_affects_both_decks() {
    let both = run([true, true], TOTAL, KEEP, |_| {});
    let id = slot(FxUnit::Master, 0);
    let with_fx = run([true, true], TOTAL, KEEP, |rig| {
        let (e, _) = gain_fx(0.5);
        install(rig, id, e);
        mix(rig, id, 1.0);
        enable(rig, id, true);
    });
    let expected: Vec<f32> = both.iter().map(|x| 0.5 * x).collect();
    assert_close(&with_fx, &expected, 1e-4, "master halved");
}

#[test]
fn slots_run_in_series_in_order() {
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let out = run([true, false], TOTAL, KEEP, |rig| {
        for (i, g) in [0.5_f32, 0.5, 1.0].into_iter().enumerate() {
            let id = slot(FxUnit::Deck(deck(0)), i);
            let (e, _) = gain_fx(g);
            install(rig, id, e);
            mix(rig, id, 1.0);
            enable(rig, id, true);
        }
    });
    let expected: Vec<f32> = base.iter().map(|x| 0.25 * x).collect();
    assert_close(&out, &expected, 1e-4, "0.5 x 0.5 x 1");
}

#[test]
fn disabled_and_dry_slots_are_transparent() {
    let base = run([true, true], TOTAL, KEEP, |_| {});
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let off = run([true, true], TOTAL, KEEP, |rig| {
        let (e, p) = gain_fx(0.0);
        install(rig, id, e);
        mix(rig, id, 1.0);
        drop(p);
    });
    assert_eq!(off, base, "installed but disabled is bit exact");
    let dry = run([true, true], TOTAL, KEEP, |rig| {
        let (e, _) = gain_fx(0.0);
        install(rig, id, e);
        mix(rig, id, 0.0);
        enable(rig, id, true);
    });
    assert_close(&dry, &base, 1e-5, "wet 0 is dry");
}

#[test]
fn wet_mix_is_equal_power() {
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let id = slot(FxUnit::Deck(deck(0)), 0);
    // Gain effect at 0: wet is silence, so the output is the dry part, cos(pi/4).
    let out = run([true, false], TOTAL, KEEP, |rig| {
        let (e, _) = gain_fx(0.0);
        install(rig, id, e);
        mix(rig, id, 0.5);
        enable(rig, id, true);
    });
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let expected: Vec<f32> = base.iter().map(|x| k * x).collect();
    assert_close(&out, &expected, 1e-4, "dry at half mix");
}

#[test]
fn unit_status_reports_enabled_active_and_level() {
    let id = slot(FxUnit::Deck(deck(0)), 1);
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 4.0), None);
    let (e, _) = gain_fx(1.0);
    install(&mut rig, id, e);
    mix(&mut rig, id, 1.0);
    rig.play(0, true);
    rig.step(BLOCK);
    let st = status(&rig, id);
    assert!(st.installed && !st.enabled && !st.active);
    assert_eq!(
        rig.r.core().fx_unit_status(FxUnit::Deck(deck(0))).level,
        0.0
    );

    enable(&mut rig, id, true);
    rig.step(2 * BLOCK);
    let st = status(&rig, id);
    assert!(st.enabled && st.active && st.level > 0.3, "{st:?}");
    let unit = rig.r.core().fx_unit_status(FxUnit::Deck(deck(0)));
    assert!(unit.enabled && unit.active && unit.level > 0.3, "{unit:?}");
    assert!(!rig.r.core().fx_unit_status(FxUnit::Master).active);

    enable(&mut rig, id, false);
    rig.step(2 * BLOCK);
    let st = status(&rig, id);
    assert!(!st.enabled && !st.active, "gain has no tail: {st:?}");
}

#[test]
fn telemetry_carries_unit_state() {
    let deck_slot = slot(FxUnit::Deck(deck(0)), 0);
    let master_slot = slot(FxUnit::Master, 0);
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 4.0), None);
    let (g, _) = gain_fx(1.0);
    install(&mut rig, deck_slot, g);
    let (echo, _) = fx(Kind::Echo);
    install(&mut rig, master_slot, echo);
    mix(&mut rig, deck_slot, 1.0);
    enable(&mut rig, deck_slot, true);
    enable(&mut rig, master_slot, true);
    rig.play(0, true);
    rig.step(4 * BLOCK);
    let t = rig.telemetry();
    assert!(t.fx_decks[0].enabled && t.fx_decks[0].active && t.fx_decks[0].level > 0.3);
    assert!(!t.fx_decks[1].enabled && !t.fx_decks[1].active);
    assert!(t.fx_master.enabled && t.fx_master.active && t.fx_master.level > 0.0);
    assert!(
        t.fx_sends
            .iter()
            .all(|u| !u.enabled && !u.active && u.level == 0.0)
    );

    // Disabled with a tail: not enabled, still active while it rings.
    enable(&mut rig, master_slot, false);
    rig.step(2 * BLOCK);
    let t = rig.telemetry();
    assert!(
        !t.fx_master.enabled && t.fx_master.active,
        "{:?}",
        t.fx_master
    );
}

// ----- Send / return -----

fn send(rig: &mut Rig, deck_index: u8, bus: u8, level: f32) {
    rig.send(EngineCommand::SetFxSend {
        deck: deck(deck_index),
        bus,
        level: Gain(level),
    });
}

fn send_rig(extra: impl FnOnce(&mut Rig)) -> Vec<f32> {
    run([true, false], TOTAL, KEEP, |rig| {
        let id = slot(FxUnit::Send(0), 0);
        let (e, _) = gain_fx(0.5);
        install(rig, id, e);
        enable(rig, id, true);
        extra(rig);
    })
}

#[test]
fn send_return_mixes_the_returned_signal() {
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let out = send_rig(|rig| send(rig, 0, 0, 1.0));
    let expected: Vec<f32> = base.iter().map(|x| 1.5 * x).collect();
    assert_close(&out, &expected, 1e-4, "dry plus 0.5 return");

    let half = send_rig(|rig| send(rig, 0, 0, 0.5));
    let expected: Vec<f32> = base.iter().map(|x| 1.25 * x).collect();
    assert_close(&half, &expected, 1e-4, "send level scales the return");

    let none = send_rig(|_| {});
    assert_close(&none, &base, 1e-6, "no send level, no return");

    let other_bus = send_rig(|rig| send(rig, 0, 1, 1.0));
    assert_close(&other_bus, &base, 1e-6, "bus 1 has no effect");
}

#[test]
fn disabled_send_unit_returns_nothing() {
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let out = run([true, false], TOTAL, KEEP, |rig| {
        let id = slot(FxUnit::Send(0), 0);
        let (e, _) = gain_fx(1.0);
        install(rig, id, e);
        send(rig, 0, 0, 1.0);
    });
    assert_eq!(out, base, "an idle send chain must not return dry signal");
}

#[test]
fn send_tap_pre_and_post_fader() {
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let fader_down = |rig: &mut Rig| {
        rig.send(EngineCommand::SetChannelFader {
            deck: deck(0),
            position: 0.0,
        });
    };
    let post = send_rig(|rig| {
        send(rig, 0, 0, 1.0);
        fader_down(rig);
    });
    assert!(peak(&post) < 1e-4, "post-fader send follows the fader");

    let pre = send_rig(|rig| {
        assert!(rig.r.handle().set_fx_send_tap(0, SendTap::PreFader));
        send(rig, 0, 0, 1.0);
        fader_down(rig);
    });
    let expected: Vec<f32> = base.iter().map(|x| 0.5 * x).collect();
    assert_close(&pre, &expected, 1e-4, "pre-fader send ignores the fader");
}

#[test]
fn send_follows_the_insert_chain_on_the_channel() {
    // Pre-fader send is tapped after the deck insert: deck gain 0.5, return 0.5.
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let out = send_rig(|rig| {
        let id = slot(FxUnit::Deck(deck(0)), 0);
        let (e, _) = gain_fx(0.5);
        install(rig, id, e);
        mix(rig, id, 1.0);
        enable(rig, id, true);
        assert!(rig.r.handle().set_fx_send_tap(0, SendTap::PreFader));
        send(rig, 0, 0, 1.0);
    });
    // main 0.5x plus return 0.5 * 0.5x.
    let expected: Vec<f32> = base.iter().map(|x| 0.75 * x).collect();
    assert_close(&out, &expected, 1e-4, "insert then send");
}

#[test]
fn send_bus_bounds() {
    assert_eq!(FX_SEND_BUSES, 2);
    let mut rig = Rig::new(2);
    assert!(
        !rig.r
            .handle()
            .set_fx_send_tap(FX_SEND_BUSES as u8, SendTap::PreFader)
    );
}

// ----- Enable and disable -----

#[test]
fn enable_and_disable_are_click_free() {
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 6.0), None);
    let (e, _) = gain_fx(0.0);
    install(&mut rig, id, e);
    mix(&mut rig, id, 1.0);
    rig.play(0, true);
    rig.step(4 * BLOCK);

    // The 440 Hz sine at 0.5 moves at most 0.029 per frame by itself.
    let natural = max_step(&rig.step(4 * BLOCK));
    assert!(natural < 0.03, "{natural}");

    enable(&mut rig, id, true);
    let on = rig.step(8 * BLOCK);
    assert!(max_step(&on) < 0.06, "enable step {}", max_step(&on));
    assert!(
        peak(&on[on.len() - BLOCK..]) < 1e-5,
        "fully wet (muted) after the fade"
    );

    enable(&mut rig, id, false);
    let off = rig.step(8 * BLOCK);
    assert!(max_step(&off) < 0.06, "disable step {}", max_step(&off));
    assert!(peak(&off[off.len() - BLOCK..]) > 0.45, "dry again");
    assert!(!status(&rig, id).active, "no tail: asleep after the fade");
}

#[test]
fn effect_with_tail_rings_out_after_disable() {
    let id = slot(FxUnit::Master, 0);
    let mut rig = Rig::new(2);
    // 0.2 s of sound, then the deck runs dry.
    rig.load(0, sine(440.0, 0.2), None);
    let (e, probe) = fx(Kind::Echo);
    install(&mut rig, id, e);
    mix(&mut rig, id, 1.0);
    enable(&mut rig, id, true);
    rig.play(0, true);
    rig.step(SR as usize / 10);
    enable(&mut rig, id, false);
    let out = rig.step(SR as usize / 2);
    // Past the end of the track (0.2 s) only the tail is left.
    let late = &out[(SR as usize / 5) * 2..];
    assert!(rms(late) > 1e-3, "tail audible, rms {}", rms(late));
    assert!(status(&rig, id).active, "still ringing at 0.6 s");

    // Echo decays 0.9 per 480 frames: well under -90 dB after a few seconds.
    let mut ticks = 0;
    while status(&rig, id).active && ticks < 400 {
        rig.step(BLOCK);
        ticks += 1;
    }
    assert!(!status(&rig, id).active, "tail ended");
    assert!(ticks < 300, "asleep after {ticks} blocks");
    assert!(peak(&rig.step(BLOCK)) < 1e-4);

    // Enabling again starts from a clean effect.
    let resets = probe.resets.load(Ordering::Relaxed);
    enable(&mut rig, id, true);
    rig.step(BLOCK);
    assert_eq!(probe.resets.load(Ordering::Relaxed), resets + 1);
}

// ----- Context -----

#[test]
fn context_carries_tempo_phase_division_and_play_state() {
    let deck_slot = slot(FxUnit::Deck(deck(0)), 0);
    let master_slot = slot(FxUnit::Master, 2);
    let send_slot = slot(FxUnit::Send(1), 0);
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 6.0);
    let (a, pa) = gain_fx(1.0);
    let (m, pm) = gain_fx(1.0);
    let (s, ps) = gain_fx(1.0);
    install(&mut rig, deck_slot, a);
    install(&mut rig, master_slot, m);
    install(&mut rig, send_slot, s);
    for id in [deck_slot, master_slot, send_slot] {
        enable(&mut rig, id, true);
    }
    send(&mut rig, 0, 1, 1.0);
    rig.step(BLOCK);
    assert!(!pa.playing.load(Ordering::Relaxed));
    assert_eq!(f64_of(&pa.bpm), 120.0, "stopped deck still knows its tempo");

    rig.play(0, true);
    rig.step(SR as usize / 2); // quarter... one beat at 120 BPM
    for (name, p) in [("deck", &pa), ("master", &pm), ("send", &ps)] {
        assert!(p.playing.load(Ordering::Relaxed), "{name} playing");
        assert!((f64_of(&p.bpm) - 120.0).abs() < 1e-6, "{name} bpm");
        let beat = f64_of(&p.beat);
        assert!(
            (0.0..4.0).contains(&beat) && beat > 0.5,
            "{name} beat {beat}"
        );
    }

    // Tempo fader: +8 % range, +4 % position.
    rig.send(EngineCommand::SetTempoFader {
        deck: deck(0),
        position: -0.5,
    });
    rig.step(BLOCK);
    let bpm = f64_of(&pa.bpm);
    assert!(
        bpm != 120.0 && (bpm - f64_of(&pm.bpm)).abs() < 1e-9,
        "{bpm}"
    );

    // Beat division, with clamping.
    for id in [deck_slot, master_slot, send_slot] {
        assert_eq!(status(&rig, id).beat_division, 1.0);
    }
    rig.send(EngineCommand::SetFxBeatDivision {
        slot: deck_slot,
        beats: Beats(0.25),
    });
    rig.send(EngineCommand::SetFxBeatDivision {
        slot: master_slot,
        beats: Beats(1.0e6),
    });
    rig.send(EngineCommand::SetFxBeatDivision {
        slot: send_slot,
        beats: Beats(-2.0),
    });
    rig.step(BLOCK);
    assert_eq!(f64_of(&pa.division), 0.25);
    assert_eq!(f64_of(&pm.division), 64.0);
    assert_eq!(f64_of(&ps.division), 1.0, "negative division ignored");

    rig.play(0, false);
    rig.step(BLOCK);
    assert!(!pm.playing.load(Ordering::Relaxed));
}

#[test]
fn master_context_follows_the_leader() {
    let master_slot = slot(FxUnit::Master, 0);
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 6.0);
    rig.load_click(1, 90.0, 6.0);
    let (m, pm) = gain_fx(1.0);
    install(&mut rig, master_slot, m);
    enable(&mut rig, master_slot, true);
    rig.play(0, true);
    rig.play(1, true);
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(1)),
    });
    rig.step(2 * BLOCK);
    assert!((f64_of(&pm.bpm) - 90.0).abs() < 1e-6);
    rig.send(EngineCommand::SetLeader {
        deck: Some(deck(0)),
    });
    rig.step(2 * BLOCK);
    assert!((f64_of(&pm.bpm) - 120.0).abs() < 1e-6);
}

#[test]
fn parameters_are_clamped_and_unknown_ids_ignored() {
    let id = slot(FxUnit::Deck(deck(1)), 2);
    let mut rig = Rig::new(2);
    let (e, p) = gain_fx(1.0);
    install(&mut rig, id, e);
    let set = |rig: &mut Rig, param: u32, value: f32| {
        rig.send(EngineCommand::SetFxParam {
            slot: id,
            param: ParamId(param),
            value,
        });
        rig.step(BLOCK);
    };
    let last = || f32::from_bits(p.last_param.load(Ordering::Relaxed));
    set(&mut rig, 0, 5.0);
    assert_eq!(last(), 1.0);
    set(&mut rig, 0, -3.0);
    assert_eq!(last(), 0.0);
    set(&mut rig, 0, 0.25);
    assert_eq!(last(), 0.25);
    let calls = p.param_calls.load(Ordering::Relaxed);
    set(&mut rig, 7, 0.5);
    set(&mut rig, 0, f32::NAN);
    assert_eq!(p.param_calls.load(Ordering::Relaxed), calls, "ignored");
    assert_eq!(last(), 0.25);

    // Mix is clamped too.
    mix(&mut rig, id, 7.0);
    rig.step(BLOCK);
    assert_eq!(status(&rig, id).wet, 1.0);
    mix(&mut rig, id, -1.0);
    rig.step(BLOCK);
    assert_eq!(status(&rig, id).wet, 0.0);
}

// ----- Latency -----

#[test]
fn dry_path_is_delay_compensated() {
    let id = slot(FxUnit::Master, 0);
    let latency = 100;
    let mut rig = Rig::new(2);
    rig.load(
        0,
        {
            // One impulse at frame 1000, then silence.
            let mut s = vec![0.0_f32; 4000 * 2];
            s[2000] = 0.5;
            s[2001] = 0.5;
            TrackBuffer::from_vec(s).expect("track")
        },
        None,
    );
    let (e, _) = fx(Kind::Latency(latency));
    install(&mut rig, id, e);
    assert_eq!(
        status_after_install(&mut rig, id).latency_frames,
        latency as u32
    );

    let first_peak = |out: &[f32]| -> Vec<usize> {
        left(out)
            .iter()
            .enumerate()
            .filter(|(_, v)| v.abs() > 1e-3)
            .map(|(i, _)| i)
            .collect()
    };

    // Enabled, 50 % wet: dry and wet impulses line up one latency later.
    mix(&mut rig, id, 0.5);
    enable(&mut rig, id, true);
    rig.step(BLOCK);
    rig.seek(0, 0);
    rig.play(0, true);
    let out = rig.step(4 * BLOCK);
    let at = first_peak(&out);
    assert_eq!(at.len(), 1, "one aligned impulse, got {at:?}");
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let h = left(&out)[at[0]];
    assert!((h - 0.5 * 2.0 * k).abs() < 1e-3, "height {h}");
    let enabled_at = at[0];

    // Bypassed: still one latency late, so toggling never shifts timing.
    enable(&mut rig, id, false);
    rig.step(4 * BLOCK);
    rig.play(0, false);
    rig.seek(0, 0);
    rig.step(BLOCK);
    rig.play(0, true);
    let out = rig.step(4 * BLOCK);
    let at = first_peak(&out);
    assert_eq!(at.len(), 1, "{at:?}");
    assert_eq!(at[0], enabled_at, "same position enabled and bypassed");
    assert!((left(&out)[at[0]] - 0.5).abs() < 1e-3);
}

fn status_after_install(rig: &mut Rig, id: FxSlotId) -> crate::FxSlotStatus {
    rig.step(BLOCK);
    status(rig, id)
}

// ----- Hand-off, swap, deferred drop -----

#[test]
fn effects_are_swapped_and_dropped_off_the_audio_thread() {
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let base = run([true, false], TOTAL, KEEP, |_| {});
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 4.0), None);
    let (first, p1) = gain_fx(0.5);
    install(&mut rig, id, first);
    mix(&mut rig, id, 1.0);
    enable(&mut rig, id, true);
    rig.play(0, true);
    rig.step(8 * BLOCK);
    assert!(p1.blocks.load(Ordering::Relaxed) > 0);

    let (second, p2) = gain_fx(0.25);
    install(&mut rig, id, second);
    rig.step(2 * BLOCK); // under assert_no_alloc: a drop here aborts
    assert!(
        !p1.dropped.load(Ordering::Relaxed),
        "audio thread must not drop"
    );
    assert_eq!(rig.r.handle().reclaim(), 1);
    assert!(p1.dropped.load(Ordering::Relaxed), "dropped by reclaim");
    assert!(!p2.dropped.load(Ordering::Relaxed));

    let out = rig.step(TOTAL);
    let tail = &out[(TOTAL - KEEP) * 2..];
    assert!(peak(tail) > 0.05);

    // Clear returns the effect and the slot turns transparent.
    assert!(rig.r.handle().clear_effect(id));
    rig.step(BLOCK);
    assert_eq!(rig.r.handle().reclaim(), 1);
    assert!(p2.dropped.load(Ordering::Relaxed));
    assert!(!status(&rig, id).installed);
    let out = rig.step(TOTAL);
    let again: Vec<f32> = out[(TOTAL - KEEP) * 2..].to_vec();
    assert!(rms(&again) > 0.3);
    drop(base);
}

#[test]
fn install_is_prepared_for_the_engine() {
    let id = slot(FxUnit::Master, 1);
    let mut rig = Rig::new(2);
    let (e, p) = gain_fx(1.0);
    install(&mut rig, id, e);
    assert_eq!(p.prepared.load(Ordering::Relaxed), 1);
    rig.step(BLOCK);
    assert!(status(&rig, id).installed);
}

#[test]
fn full_hand_off_ring_gives_the_effect_back() {
    let id = slot(FxUnit::Master, 0);
    let mut rig = Rig::new(2);
    let mut given_back = 0;
    for _ in 0..crate::FX_HANDOFF_RING + 2 {
        let (e, _) = gain_fx(1.0);
        if rig.r.handle().install_effect(id, e).is_err() {
            given_back += 1;
        }
    }
    assert_eq!(given_back, 2);
    // The audio thread takes them one by one as the return ring allows.
    for _ in 0..4 {
        rig.step(BLOCK);
        rig.r.handle().reclaim();
    }
    assert!(status(&rig, id).installed);
}

#[test]
fn every_slot_of_every_unit_is_addressable() {
    let mut rig = Rig::new(4);
    let mut units = vec![FxUnit::Master];
    units.extend((0..MAX_DECKS as u8).filter_map(|d| deck_opt(d).map(FxUnit::Deck)));
    units.extend((0..FX_SEND_BUSES as u8).map(FxUnit::Send));
    for unit in units {
        for i in 0..FX_SLOTS_PER_UNIT {
            let id = slot(unit, i);
            let (e, _) = gain_fx(1.0);
            install(&mut rig, id, e);
            rig.step(BLOCK);
            assert!(status(&rig, id).installed, "{unit:?} {i}");
            rig.r.handle().reclaim();
        }
    }
}

fn deck_opt(i: u8) -> Option<opendeck_contracts::DeckId> {
    opendeck_contracts::DeckId::new(i)
}

// ----- CPU -----

#[test]
fn cpu_two_decks_six_slots_plus_master_six_slots() {
    let mut r = OfflineRenderer::new(crate::tests_support::config(2)).expect("config");
    for d in 0..2u8 {
        r.handle()
            .load_track(deck(d), sine(300.0 + 200.0 * f64::from(d), 12.0), None)
            .expect("ring");
    }
    let mut units = vec![FxUnit::Master];
    units.extend((0..2u8).map(|d| FxUnit::Deck(deck(d))));
    for unit in units {
        for i in 0..FX_SLOTS_PER_UNIT {
            let id = slot(unit, i);
            let (e, _) = gain_fx(0.9);
            r.handle()
                .install_effect(id, e)
                .map_err(|_| ())
                .expect("ring");
            r.handle()
                .send(EngineCommand::SetFxMix { slot: id, wet: 1.0 })
                .expect("ring");
            r.handle()
                .send(EngineCommand::SetFxEnabled { slot: id, on: true })
                .expect("ring");
            r.handle().reclaim();
            r.render(BLOCK);
        }
    }
    for d in 0..2u8 {
        r.handle()
            .send(EngineCommand::SetPlaying {
                deck: deck(d),
                playing: true,
            })
            .expect("ring");
    }
    let before = r.stats();
    let out = r.render(SR as usize * 5);
    assert!(peak(&out) > 0.1);
    let mut s = r.stats();
    s.blocks -= before.blocks;
    s.frames -= before.frames;
    s.total -= before.total;
    let fraction = s.realtime_fraction(SR);
    eprintln!(
        "fx cpu: 2 decks x {n} + master x {n} slots: {:.2} % of realtime, worst block {:?}",
        fraction * 100.0,
        s.worst_block,
        n = FX_SLOTS_PER_UNIT
    );
    if !cfg!(debug_assertions) {
        assert!(fraction < 0.25, "{fraction}");
    }
}

// ----- Tempo source -----

#[test]
fn tapped_tempo_runs_its_own_beat_counter() {
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 6.0);
    let (e, p) = gain_fx(1.0);
    install(&mut rig, id, e);
    enable(&mut rig, id, true);
    rig.step(BLOCK);
    assert_eq!(f64_of(&p.bpm), 120.0, "auto: the deck");

    assert!(
        rig.r
            .handle()
            .set_fx_tempo_source(FxUnit::Deck(deck(0)), FxTempoSource::Tap(Bpm(100.0)))
    );
    rig.step(10 * BLOCK);
    assert_eq!(f64_of(&p.bpm), 100.0);
    // Ten blocks: the last starts at beat 9 * 512 * 100 / 60 / 48000.
    let expected = 9.0 * BLOCK as f64 * 100.0 / 60.0 / f64::from(SR);
    assert!(
        (f64_of(&p.beat) - expected).abs() < 1e-9,
        "{}",
        f64_of(&p.beat)
    );
    assert!(
        !p.playing.load(Ordering::Relaxed),
        "play flag still from the deck"
    );

    // A tapped tempo must be finite and positive.
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            !rig.r
                .handle()
                .set_fx_tempo_source(FxUnit::Deck(deck(0)), FxTempoSource::Tap(Bpm(bad)))
        );
    }
    rig.step(BLOCK);
    assert_eq!(f64_of(&p.bpm), 100.0, "rejected taps leave the source");

    // Back to auto.
    assert!(
        rig.r
            .handle()
            .set_fx_tempo_source(FxUnit::Deck(deck(0)), FxTempoSource::Auto)
    );
    rig.step(BLOCK);
    assert_eq!(f64_of(&p.bpm), 120.0);
}

#[test]
fn unit_can_follow_another_deck() {
    let on_deck1 = slot(FxUnit::Deck(deck(1)), 0);
    let on_deck0 = slot(FxUnit::Deck(deck(0)), 0);
    let mut rig = Rig::new(2);
    rig.load_click(0, 120.0, 6.0);
    rig.load_click(1, 90.0, 6.0);
    let (a, pa) = gain_fx(1.0);
    let (b, pb) = gain_fx(1.0);
    install(&mut rig, on_deck0, a);
    install(&mut rig, on_deck1, b);
    enable(&mut rig, on_deck0, true);
    enable(&mut rig, on_deck1, true);
    rig.play(0, true);
    rig.step(4 * BLOCK);
    assert_eq!(f64_of(&pb.bpm), 90.0);
    assert!(!pb.playing.load(Ordering::Relaxed));

    // "1&2": both units on deck 0's tempo and phase.
    assert!(
        rig.r
            .handle()
            .set_fx_tempo_source(FxUnit::Deck(deck(1)), FxTempoSource::Deck(deck(0)))
    );
    rig.step(4 * BLOCK);
    assert_eq!(f64_of(&pb.bpm), 120.0);
    assert_eq!(f64_of(&pb.beat), f64_of(&pa.beat));
    assert!(pb.playing.load(Ordering::Relaxed));
}

// ----- Timed engage and release -----

#[test]
fn timed_enable_starts_on_the_exact_frame() {
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let at = 5000_usize;
    let render = |fx_on: bool| {
        let mut rig = Rig::new(2);
        rig.load(0, sine(440.0, 4.0), None);
        let start = 512; // frames rendered by the load
        if fx_on {
            let (e, _) = gain_fx(0.0);
            install(&mut rig, id, e);
            mix(&mut rig, id, 1.0);
            assert!(rig.r.handle().schedule_fx_enabled(
                id,
                true,
                Frames((start + at) as u64),
                true
            ));
        }
        rig.play(0, true);
        left(&rig.step(10_000))
    };
    let base = render(false);
    let out = render(true);
    assert_eq!(base[..at], out[..at], "bit exact before the frame");
    let first = (0..base.len())
        .find(|&i| base[i] != out[i])
        .expect("effect engaged");
    assert!((at..at + 3).contains(&first), "first change at {first}");
    // 1 ms ramp: silent by 64 frames later.
    assert!(peak(&out[at + 64..at + 400]) < 1e-6);
    assert!(peak(&base[at + 64..at + 400]) > 0.1);
}

#[test]
fn release_fx_bypasses_the_main_effect_and_lets_the_tail_ring() {
    let main = slot(FxUnit::Master, 0);
    let release = slot(FxUnit::Master, 1);
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 0.5), None);
    let (m, _) = gain_fx(0.0); // mutes while enabled
    let (r, pr) = fx(Kind::Echo);
    install(&mut rig, main, m);
    install(&mut rig, release, r);
    mix(&mut rig, main, 1.0);
    mix(&mut rig, release, 1.0);
    enable(&mut rig, main, true);
    rig.play(0, true);
    let mut now = 512_u64;
    let pre = rig.step(4000);
    now += 4000;
    assert!(peak(&pre[2000 * 2..]) < 1e-4, "muted before the release");

    let at = now + 500;
    assert!(
        rig.r
            .handle()
            .release_fx(main, release, Frames(at), Frames(2400))
    );
    let during = rig.step(3000);
    now += 3000;
    assert!(
        peak(&during[1000 * 2..2500 * 2]) > 0.05,
        "release effect audible"
    );
    assert!(pr.blocks.load(Ordering::Relaxed) > 0);
    assert!(!status(&rig, main).enabled);

    // Run past the end of the track (frame 24000 + 512): only the tail is left.
    let to_go = 24_512 + 6_000 - now as usize;
    let out = rig.step(to_go);
    let window = &out[out.len() - 4_000 * 2..out.len() - 2_000 * 2];
    assert!(rms(window) > 1e-4, "tail rings, rms {}", rms(window));
    assert!(status(&rig, release).active, "release effect still ringing");
    assert!(!status(&rig, main).active, "main has no tail");
    let mut ticks = 0;
    while status(&rig, release).active && ticks < 600 {
        rig.step(BLOCK);
        ticks += 1;
    }
    assert!(!status(&rig, release).active, "release tail ends");
}

// ----- Host rules for capture style effects -----

#[test]
fn effect_is_reset_on_every_engage_even_during_a_tail() {
    let id = slot(FxUnit::Master, 0);
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 4.0), None);
    let (e, p) = fx(Kind::Echo);
    install(&mut rig, id, e);
    enable(&mut rig, id, true);
    rig.play(0, true);
    rig.step(4 * BLOCK);
    let after_first = p.resets.load(Ordering::Relaxed);
    assert_eq!(after_first, 1);

    enable(&mut rig, id, false);
    rig.step(2 * BLOCK);
    assert!(status(&rig, id).active, "still ringing");
    enable(&mut rig, id, true);
    rig.step(BLOCK);
    assert_eq!(p.resets.load(Ordering::Relaxed), after_first + 1);

    // Staying enabled does not reset again.
    rig.step(4 * BLOCK);
    assert_eq!(p.resets.load(Ordering::Relaxed), after_first + 1);
}

#[test]
fn timed_engage_resets_the_effect_on_its_frame() {
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let mut rig = Rig::new(2);
    rig.load(0, sine(440.0, 4.0), None);
    let (e, p) = gain_fx(1.0);
    install(&mut rig, id, e);
    rig.play(0, true);
    rig.step(2 * BLOCK);
    assert_eq!(p.blocks.load(Ordering::Relaxed), 0, "asleep, never run");

    // Engage 200 frames into the next block (engine frame 1536 + 200).
    assert!(
        rig.r
            .handle()
            .schedule_fx_enabled(id, true, Frames(1536 + 200), true)
    );
    rig.step(BLOCK);
    assert_eq!(p.resets.load(Ordering::Relaxed), 1);
    // The effect ran only from the engage frame: one segment of 312 frames
    // after the reset, plus the 200 frames before it (host split).
    assert_eq!(p.blocks.load(Ordering::Relaxed), 2);
}

#[test]
fn latency_of_a_pitch_sized_lookahead_is_compensated() {
    // 960 frames is what the pitch effect reports at 48 kHz.
    let id = slot(FxUnit::Deck(deck(0)), 0);
    let mut rig = Rig::new(2);
    let (e, _) = fx(Kind::Latency(960));
    install(&mut rig, id, e);
    rig.step(BLOCK);
    assert_eq!(status(&rig, id).latency_frames, 960);
}
