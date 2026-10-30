//! Crate-level tests: catalog and document, factory, presets, per-effect
//! stress under `assert_no_alloc`, tails, beat-sync exactness, filter and
//! behaviour checks, the reference slot host and a CPU table.

use crate::catalog::{self, CATALOG, DOC_BEGIN, DOC_END, MIN_BPM};
use crate::preset::{self, ChainPreset, ChainSlot};
use crate::slot::FxSlot;
use crate::{BuiltinFactory, PROVIDER_ID};
use opendeck_contracts::fx::{
    Effect, EffectFactory, EffectKey, EffectRegistry, ParamId, ProcessContext, StereoBufferMut,
};
use opendeck_contracts::units::{Beats, Bpm, SampleRate};

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const RATES: [u32; 3] = [44_100, 48_000, 96_000];
const BLOCKS: [usize; 3] = [1, 64, 1024];

// --- helpers ---------------------------------------------------------------

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 40) as f32 / 8_388_608.0) - 1.0
    }
}

fn noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut g = Lcg(seed);
    (0..n).map(|_| amp * g.next()).collect()
}

fn sine(n: usize, sr: u32, freq: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / sr as f32).sin())
        .collect()
}

fn ctx(sr: u32, bpm: Option<f64>, beat: Option<f64>, div: f64) -> ProcessContext {
    ProcessContext {
        sample_rate: SampleRate(sr),
        bpm: bpm.map(Bpm),
        beat_position: beat.map(Beats),
        beat_division: Beats(div),
        playing: true,
    }
}

fn make(id: &str, sr: u32, block: usize) -> Box<dyn Effect> {
    let mut fx = BuiltinFactory.create(id).expect("known effect");
    fx.prepare(SampleRate(sr), block);
    fx
}

fn set_plain(fx: &mut dyn Effect, key: &str, plain: f32) {
    let spec = catalog::find(&fx.info().key.id).expect("catalog entry");
    let i = spec.index_of(key).expect("param key");
    let d = spec.params[i].descriptor(i);
    fx.set_parameter(d.id, d.to_normalized(plain));
}

fn process_guarded(fx: &mut dyn Effect, l: &mut [f32], r: &mut [f32], c: &ProcessContext) {
    let io = StereoBufferMut::new(l, r).expect("same length");
    assert_no_alloc::assert_no_alloc(|| fx.process(io, c));
}

/// Runs `input` through `fx` in blocks at a fixed tempo with a moving beat
/// position. Returns the output.
fn run(
    fx: &mut dyn Effect,
    sr: u32,
    block: usize,
    in_l: &[f32],
    in_r: &[f32],
    bpm: Option<f64>,
    div: f64,
) -> (Vec<f32>, Vec<f32>) {
    let mut ol = in_l.to_vec();
    let mut or = in_r.to_vec();
    let mut pos = 0;
    while pos < ol.len() {
        let end = (pos + block).min(ol.len());
        let beat = bpm.map(|b| pos as f64 * b / 60.0 / f64::from(sr));
        let c = ctx(sr, bpm, beat, div);
        process_guarded(fx, &mut ol[pos..end], &mut or[pos..end], &c);
        pos = end;
    }
    (ol, or)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, v| m.max(v.abs()))
}

fn zero_crossings(x: &[f32]) -> usize {
    x.windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count()
}

// --- catalog and document --------------------------------------------------

const EXPECTED_KEYS: [&str; 17] = [
    "delay",
    "echo",
    "ping_pong",
    "spiral",
    "reverb",
    "helix",
    "flanger",
    "phaser",
    "filter",
    "trans",
    "roll",
    "reverse_roll",
    "pitch",
    "vinyl_brake",
    "noise",
    "bit_crusher",
    "colour_filter",
];

#[test]
fn catalog_is_consistent() {
    let ids: Vec<&str> = CATALOG.iter().map(|e| e.id).collect();
    assert_eq!(ids, EXPECTED_KEYS);
    for e in CATALOG {
        assert!(e.params.len() <= crate::core::MAXP, "{}", e.id);
        assert!(e.index_of(e.primary).is_some(), "{} primary", e.id);
        let info = e.info();
        assert_eq!(info.beat_synced, e.sync.is_some());
        for (i, d) in info.params.iter().enumerate() {
            assert!(d.is_valid(), "{}:{}", e.id, e.params[i].key);
            assert_eq!(d.id, ParamId(i as u32));
        }
        let mut keys: Vec<&str> = e.params.iter().map(|p| p.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), e.params.len(), "duplicate key in {}", e.id);
        if let Some(s) = e.sync {
            assert!(s.min_beats <= s.default_beats && s.default_beats <= s.max_beats);
        }
        assert!(!e.presets.is_empty(), "{} has no presets", e.id);
        let mut names: Vec<&str> = e.presets.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), e.presets.len(), "duplicate preset in {}", e.id);
        for pr in e.presets {
            assert!((0.0..=1.0).contains(&pr.wet));
            if let Some(s) = e.sync {
                assert!(pr.division >= s.min_beats && pr.division <= s.max_beats);
            }
            for (k, v) in pr.params {
                let i = e.index_of(k).unwrap_or_else(|| panic!("{}: key {k}", e.id));
                let p = &e.params[i];
                assert!(
                    *v >= p.min && *v <= p.max,
                    "{}:{} {}",
                    e.id,
                    p.name,
                    pr.name
                );
            }
        }
    }
}

const DOC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/fx-catalog.md");

#[test]
fn doc_tables_match_catalog_data() {
    let generated = catalog::render_markdown();
    let doc = std::fs::read_to_string(DOC_PATH).expect("docs/fx-catalog.md exists");
    let begin = doc.find(DOC_BEGIN).expect("begin marker in the doc");
    let end_marker = doc.find(DOC_END).expect("end marker in the doc") + DOC_END.len();
    if std::env::var_os("OPENDECK_BLESS_FX_CATALOG").is_some() {
        let mut updated = doc[..begin].to_owned();
        updated.push_str(generated.trim_end());
        updated.push_str(&doc[end_marker..]);
        std::fs::write(DOC_PATH, updated).expect("write doc");
        return;
    }
    assert_eq!(
        doc[begin..end_marker].trim_end(),
        generated.trim_end(),
        "docs/fx-catalog.md is stale; run with OPENDECK_BLESS_FX_CATALOG=1 to regenerate"
    );
}

// --- factory and presets ---------------------------------------------------

#[test]
fn factory_lists_and_creates_every_effect() {
    let f = BuiltinFactory::new();
    assert_eq!(f.provider(), PROVIDER_ID);
    let infos = f.effects();
    assert_eq!(infos.len(), EXPECTED_KEYS.len());
    for (info, id) in infos.iter().zip(EXPECTED_KEYS) {
        assert_eq!(info.key.provider, "builtin");
        assert_eq!(info.key.id, id);
        let fx = f.create(id).expect("creates");
        assert_eq!(fx.info(), info);
    }
    assert!(f.create("nope").is_err());

    let mut reg = EffectRegistry::new();
    reg.register(Box::new(BuiltinFactory)).expect("register");
    let key = EffectKey {
        provider: "builtin".into(),
        id: "reverb".into(),
    };
    assert!(reg.create(&key).is_ok());
    assert_eq!(reg.effects().len(), 17);
}

#[test]
fn unprepared_effect_is_silent_not_panicking() {
    let mut fx = BuiltinFactory.create("delay").expect("creates");
    let mut l = [1.0; 8];
    let mut r = [1.0; 8];
    fx.process(
        StereoBufferMut::new(&mut l, &mut r).expect("len"),
        &ctx(48_000, Some(120.0), None, 1.0),
    );
    assert_eq!(l, [0.0; 8]);
    fx.set_parameter(ParamId(999), 0.5);
}

#[test]
fn presets_roundtrip_and_apply() {
    let all = preset::all_factory_presets();
    assert!(all.len() >= EXPECTED_KEYS.len() * 2);
    for p in &all {
        assert_eq!(p.effect.provider, "builtin");
        let spec = catalog::find(&p.effect.id).expect("effect");
        assert_eq!(p.params.len(), spec.params.len());
        for (_, v) in &p.params {
            assert!((0.0..=1.0).contains(v));
        }
        let json = preset::preset_to_json(p).expect("serialise");
        assert_eq!(&preset::preset_from_json(&json).expect("parse"), p);
        let mut fx = make(&p.effect.id, 48_000, 64);
        preset::apply_preset(fx.as_mut(), p);
    }
    // A named factory preset carries the catalog value.
    let hall = preset::factory_presets("reverb")
        .into_iter()
        .find(|p| p.name == "Hall")
        .expect("Hall");
    let spec = catalog::find("reverb").expect("spec");
    let size = spec.index_of("size").expect("size");
    let d = spec.params[size].descriptor(size);
    assert!((d.to_plain(hall.params[size].1) - 70.0).abs() < 1e-3);
    assert!(preset::factory_presets("nope").is_empty());
}

#[test]
fn chain_preset_roundtrip_and_validation() {
    let slots: Vec<ChainSlot> = ["echo", "filter", "reverb"]
        .iter()
        .map(|id| ChainSlot::from_preset(&preset::factory_presets(id)[0], true))
        .collect();
    let chain = ChainPreset {
        name: "Build".into(),
        slots,
    };
    assert_eq!(chain.validate(), Ok(()));
    let json = chain.to_json().expect("serialise");
    assert_eq!(ChainPreset::from_json(&json).expect("parse"), chain);

    let mut too_many = chain.clone();
    too_many.slots.push(too_many.slots[0].clone());
    assert!(too_many.validate().is_err());
    let mut bad = chain;
    bad.slots[1].wet = 1.5;
    assert!(bad.validate().is_err());
}

// --- per-effect stress -----------------------------------------------------

/// Input peaks at 0.5; the loudest feedback settings may add gain.
const PEAK_LIMIT: f32 = 12.0;

/// Random parameters, tempo changes, division changes; allocation-guarded.
fn stress(id: &str, sr: u32, block: usize, signal: &[f32]) {
    let mut fx = make(id, sr, block);
    let n_params = fx.info().params.len();
    let mut g = Lcg(0xdead_beef ^ u64::from(sr) ^ ((block as u64) << 32));
    let seg = (sr / 2) as usize;
    let mut pos = 0;
    let mut seg_i = 0;
    let mut max_peak = 0.0_f32;
    let mut l = vec![0.0; block];
    let mut r = vec![0.0; block];
    while pos < signal.len() {
        let bpm = if seg_i % 5 == 4 {
            None
        } else {
            Some(60.0 + ((seg_i * 37) % 100) as f64)
        };
        let div = Beats::STANDARD_SIZES[seg_i % 11].0;
        if seg_i % 2 == 1 {
            for p in 0..n_params {
                let v = 0.5 * (g.next() + 1.0);
                assert_no_alloc::assert_no_alloc(|| fx.set_parameter(ParamId(p as u32), v));
            }
        }
        let seg_end = (pos + seg).min(signal.len());
        while pos < seg_end {
            let end = (pos + block).min(seg_end);
            let n = end - pos;
            l[..n].copy_from_slice(&signal[pos..end]);
            r[..n].copy_from_slice(&signal[pos..end]);
            let beat = bpm.map(|b| pos as f64 * b / 60.0 / f64::from(sr));
            process_guarded(
                fx.as_mut(),
                &mut l[..n],
                &mut r[..n],
                &ctx(sr, bpm, beat, div),
            );
            for s in l[..n].iter().chain(r[..n].iter()) {
                assert!(
                    s.is_finite(),
                    "{id} non-finite at {pos} (sr {sr}, block {block})"
                );
            }
            max_peak = max_peak.max(peak(&l[..n])).max(peak(&r[..n]));
            pos = end;
        }
        seg_i += 1;
    }
    assert!(
        max_peak <= PEAK_LIMIT,
        "{id} peak {max_peak} (sr {sr}, block {block})"
    );
}

fn stress_all(id: &str) {
    for sr in RATES {
        let n = 10 * sr as usize;
        let noise_sig = noise(n, 0.5, 7);
        let sine_sig = sine(n, sr, 440.0, 0.5);
        for block in BLOCKS {
            stress(id, sr, block, &noise_sig);
            stress(id, sr, block, &sine_sig);
        }
    }
}

macro_rules! stress_tests {
    ($($name:ident => $id:literal),* $(,)?) => {
        $(#[test] fn $name() { stress_all($id); })*
    };
}

stress_tests! {
    stress_delay => "delay",
    stress_echo => "echo",
    stress_ping_pong => "ping_pong",
    stress_spiral => "spiral",
    stress_reverb => "reverb",
    stress_helix => "helix",
    stress_flanger => "flanger",
    stress_phaser => "phaser",
    stress_filter => "filter",
    stress_trans => "trans",
    stress_roll => "roll",
    stress_reverse_roll => "reverse_roll",
    stress_pitch => "pitch",
    stress_vinyl_brake => "vinyl_brake",
    stress_noise => "noise",
    stress_bit_crusher => "bit_crusher",
    stress_colour_filter => "colour_filter",
}

#[test]
fn silence_in_gives_silence_out() {
    for id in EXPECTED_KEYS {
        if id == "noise" {
            continue;
        }
        let mut fx = make(id, 48_000, 256);
        let z = vec![0.0; 48_000];
        let (l, r) = run(fx.as_mut(), 48_000, 256, &z, &z, Some(120.0), 0.5);
        assert!(peak(&l) < 1e-6 && peak(&r) < 1e-6, "{id}");
    }
}

#[test]
fn tails_decay_after_input_stops() {
    for e in CATALOG.iter().filter(|e| e.tail) {
        let sr = 48_000;
        let mut fx = make(e.id, sr, 512);
        let mut input = noise(sr as usize, 0.5, 3);
        input.extend(std::iter::repeat_n(0.0, 20 * sr as usize));
        let (l, _) = run(fx.as_mut(), sr, 512, &input, &input, Some(120.0), 0.5);
        let during = rms(&l[sr as usize / 2..sr as usize]);
        let last = rms(&l[l.len() - sr as usize..]);
        assert!(during > 0.0, "{} produced nothing", e.id);
        assert!(last < 1e-3, "{} tail still {last}", e.id);
    }
}

#[test]
fn reverb_tail_decays_and_size_lengthens_it() {
    let sr = 48_000;
    let mut input = noise(sr as usize / 2, 0.5, 5);
    input.extend(std::iter::repeat_n(0.0, 12 * sr as usize));
    let window = |l: &[f32], from_s: f32, len_s: f32| {
        let a = (from_s * sr as f32) as usize;
        rms(&l[a..a + (len_s * sr as f32) as usize])
    };
    let mut tails = Vec::new();
    for size in [10.0, 90.0] {
        let mut fx = make("reverb", sr, 512);
        set_plain(fx.as_mut(), "size", size);
        let (l, _) = run(fx.as_mut(), sr, 512, &input, &input, None, 1.0);
        let early = window(&l, 1.0, 0.5);
        let mid = window(&l, 3.0, 0.5);
        let late = window(&l, 8.0, 0.5);
        assert!(
            early > mid && mid > late,
            "size {size}: {early} {mid} {late}"
        );
        tails.push(mid);
    }
    assert!(tails[1] > 2.0 * tails[0], "{tails:?}");
}

// --- beat sync -------------------------------------------------------------

#[test]
fn sync_period_equals_division_times_beat_length() {
    let sync = catalog::find("delay").and_then(|e| e.sync).expect("sync");
    for (bpm, div) in [
        (120.0, 0.5),
        (90.0, 0.75),
        (128.0, 1.0),
        (174.0, 0.125),
        (70.5, 2.0),
    ] {
        let c = crate::core::Ctx {
            sr: 48_000.0,
            bpm: Some(bpm),
            beat: None,
            division: div,
            frames: 32,
        };
        let expected = Beats(div).to_seconds(Bpm(bpm)).0 * 48_000.0;
        assert_eq!(c.period_samples(&sync, 0.0), expected);
    }
    // Slower than the buffers were sized for: the period exceeds them and the
    // delay effects clamp to the line instead of overrunning.
    let c = crate::core::Ctx {
        sr: 48_000.0,
        bpm: Some(MIN_BPM / 2.0),
        beat: None,
        division: 4.0,
        frames: 32,
    };
    assert!(c.period_samples(&sync, 0.0) > 4.0 * 60.0 / MIN_BPM * 48_000.0);
}

fn impulse_at(id: &str, bpm: Option<f64>, div: f64, at: usize, len: usize) -> (Vec<f32>, Vec<f32>) {
    let sr = 48_000;
    let mut fx = make(id, sr, 64);
    let mut input = vec![0.0; len];
    input[at] = 1.0;
    run(fx.as_mut(), sr, 64, &input, &vec![0.0; len], bpm, div)
}

#[test]
fn delay_time_is_exactly_division_over_tempo() {
    // (bpm, division, expected delay in samples at 48 kHz)
    for (bpm, div, d) in [
        (120.0, 0.5, 12_000),
        (90.0, 0.75, 24_000),
        (120.0, 0.25, 6_000),
    ] {
        let (l, _) = impulse_at("delay", Some(bpm), div, 100, 30_000);
        assert_eq!(l[100 + d], 1.0, "bpm {bpm} div {div}");
        assert!(l[100 + d - 1].abs() < 1e-6);
        assert!(l[100 + d + 1].abs() < 1e-6);
    }
    // No tempo: the Time parameter (default 500 ms).
    let (l, _) = impulse_at("delay", None, 0.5, 100, 30_000);
    assert_eq!(l[100 + 24_000], 1.0);
}

#[test]
fn ping_pong_first_repeat_left_second_right() {
    let (l, r) = impulse_at("ping_pong", Some(120.0), 0.5, 100, 40_000);
    let d = 12_000;
    assert!(l[100 + d].abs() > 0.2 && r[100 + d].abs() < 1e-6);
    assert!(r[100 + 2 * d].abs() > 0.05 && l[100 + 2 * d].abs() < 1e-6);
}

#[test]
fn echo_repeats_decay() {
    let (l, _) = impulse_at("echo", Some(120.0), 0.25, 100, 40_000);
    let d = 6_000;
    let a = peak(&l[100 + d - 50..100 + d + 50]);
    let b = peak(&l[100 + 2 * d - 50..100 + 2 * d + 50]);
    assert!(a > 0.5 && b < a && b > 0.05, "{a} {b}");
}

// --- behaviour -------------------------------------------------------------

fn tone_gain(id: &str, params: &[(&str, f32)], freq: f32) -> f32 {
    let sr = 48_000;
    let mut fx = make(id, sr, 256);
    for (k, v) in params {
        set_plain(fx.as_mut(), k, *v);
    }
    let s = sine(2 * sr as usize, sr, freq, 0.5);
    let (l, _) = run(fx.as_mut(), sr, 256, &s, &s, None, 1.0);
    rms(&l[sr as usize..]) / rms(&s[sr as usize..])
}

#[test]
fn colour_filter_response() {
    // Flat at zero.
    assert!((tone_gain("colour_filter", &[("colour", 0.0)], 1000.0) - 1.0).abs() < 0.01);
    // Low-pass at -50 %: cutoff about 1.4 kHz.
    assert!(tone_gain("colour_filter", &[("colour", -50.0)], 200.0) > 0.9);
    assert!(tone_gain("colour_filter", &[("colour", -50.0)], 8000.0) < 0.01);
    // High-pass at +50 %: cutoff about 400 Hz.
    assert!(tone_gain("colour_filter", &[("colour", 50.0)], 5000.0) > 0.9);
    assert!(tone_gain("colour_filter", &[("colour", 50.0)], 50.0) < 0.02);
}

#[test]
fn sweep_filter_lp_and_hp() {
    // Depth 0 parks the cutoff at the geometric middle of Low and High (about 1.3 kHz).
    let lp = [("depth", 0.0), ("mode", 0.0)];
    assert!(tone_gain("filter", &lp, 100.0) > 0.8);
    assert!(tone_gain("filter", &lp, 10_000.0) < 0.03);
    let hp = [("depth", 0.0), ("mode", 1.0)];
    assert!(tone_gain("filter", &hp, 10_000.0) > 0.8);
    assert!(tone_gain("filter", &hp, 100.0) < 0.03);
}

#[test]
fn trans_gates_on_the_beat_grid() {
    let sr = 48_000;
    let mut fx = make("trans", sr, 64);
    let ones = vec![1.0; sr as usize];
    // 120 bpm, division 0.5: period 0.25 s, open for the first 0.125 s.
    let (l, _) = run(fx.as_mut(), sr, 64, &ones, &ones, Some(120.0), 0.5);
    let at = |t: f32| l[(t * sr as f32) as usize];
    assert!(at(0.06) > 0.95 && at(0.19) < 0.05 && at(0.31) > 0.95 && at(0.44) < 0.05);
}

#[test]
fn roll_and_reverse_roll_repeat_the_captured_slice() {
    let sr = 48_000;
    let len = 6_000; // 120 bpm, division 0.25
    let input = noise(5 * len, 0.5, 11);
    let mut roll = make("roll", sr, 64);
    let (l, _) = run(roll.as_mut(), sr, 64, &input, &input, Some(120.0), 0.25);
    assert_eq!(&l[..len], &input[..len], "capture passes the input");
    for k in [len / 3, len / 2, 2 * len / 3] {
        assert!((l[len + k] - input[k]).abs() < 1e-6);
        assert!((l[3 * len + k] - input[k]).abs() < 1e-6);
    }
    let mut rev = make("reverse_roll", sr, 64);
    let (l, _) = run(rev.as_mut(), sr, 64, &input, &input, Some(120.0), 0.25);
    for k in [len / 3, len / 2, 2 * len / 3] {
        assert!((l[len + k] - input[len - 1 - k]).abs() < 1e-6);
    }
    // Decay lowers each repeat.
    let mut fading = make("roll", sr, 64);
    set_plain(fading.as_mut(), "decay", 50.0);
    let (l, _) = run(fading.as_mut(), sr, 64, &input, &input, Some(120.0), 0.25);
    let mid = len / 2;
    assert!(
        (l[len + mid] - input[mid]).abs() < 1e-3,
        "first repeat is full level"
    );
    assert!((l[2 * len + mid] - 0.5 * input[mid]).abs() < 1e-3);
}

#[test]
fn helix_freezes_the_recent_history() {
    let sr = 48_000;
    let block = 32;
    let mut fx = make("helix", sr, block);
    set_plain(fx.as_mut(), "hold", 0.0);
    let input = noise(sr as usize, 0.5, 13);
    let (l, _) = run(fx.as_mut(), sr, block, &input, &input, Some(120.0), 0.5);
    assert!(peak(&l) == 0.0, "silent while Hold is off");
    // Hold on: loop of 12000 frames ending at the hold moment.
    set_plain(fx.as_mut(), "hold", 1.0);
    let more = noise(30_000, 0.5, 17);
    let (out, _) = run(fx.as_mut(), sr, block, &more, &more, Some(120.0), 0.5);
    let mut history = input.clone();
    history.extend_from_slice(&more);
    let hold_at = input.len();
    for j in [4_000, 6_000, 9_000, 15_000, 21_000] {
        let expected = history[hold_at - 12_000 + (j % 12_000)];
        assert!((out[j] - expected).abs() < 1e-6, "j {j}");
    }
}

#[test]
fn vinyl_brake_slows_then_stops() {
    let sr = 48_000;
    let mut fx = make("vinyl_brake", sr, 64);
    let s = sine(2 * sr as usize, sr, 1000.0, 0.5);
    // 120 bpm, division 1: stop after 0.5 s.
    let (l, _) = run(fx.as_mut(), sr, 64, &s, &s, Some(120.0), 1.0);
    let early = zero_crossings(&l[..4800]);
    let later = zero_crossings(&l[16_800..21_600]);
    assert!(early > 120 && later < early / 2, "{early} {later}");
    assert!(peak(&l[sr as usize / 2 + 2_000..]) == 0.0);
}

#[test]
fn pitch_shifts_frequency() {
    let sr = 48_000;
    let s = sine(2 * sr as usize, sr, 1000.0, 0.5);
    for (shift, ratio) in [(0.0, 1.0), (12.0, 2.0), (-12.0, 0.5)] {
        let mut fx = make("pitch", sr, 256);
        set_plain(fx.as_mut(), "shift", shift);
        let (l, _) = run(fx.as_mut(), sr, 256, &s, &s, None, 1.0);
        let zc = zero_crossings(&l[sr as usize..]) as f32;
        let expected = 2.0 * 1000.0 * ratio;
        assert!(
            (zc - expected).abs() < 0.08 * expected,
            "shift {shift}: {zc}"
        );
        assert!(rms(&l[sr as usize..]) > 0.15);
    }
    assert_eq!(make("pitch", sr, 64).latency_frames(), 960);
}

#[test]
fn bit_crusher_quantises_and_holds() {
    let sr = 48_000;
    let mut fx = make("bit_crusher", sr, 256);
    set_plain(fx.as_mut(), "bits", 4.0);
    set_plain(fx.as_mut(), "rate", 6_000.0);
    let s = sine(sr as usize, sr, 300.0, 0.9);
    let (l, _) = run(fx.as_mut(), sr, 256, &s, &s, None, 1.0);
    let tail = &l[sr as usize / 2..];
    let mut levels: Vec<i32> = tail.iter().map(|v| (v * 8.0).round() as i32).collect();
    for (v, q) in tail.iter().zip(&levels) {
        assert!((v * 8.0 - *q as f32).abs() < 1e-4, "on the 4-bit grid");
    }
    levels.dedup();
    assert!(levels.len() < tail.len() / 8 + 8, "held at 6 kHz");
}

#[test]
fn modulation_effects_alter_the_signal() {
    for id in ["flanger", "phaser", "noise"] {
        let sr = 48_000;
        let mut fx = make(id, sr, 256);
        let n = noise(2 * sr as usize, 0.3, 21);
        let (l, _) = run(fx.as_mut(), sr, 256, &n, &n, Some(120.0), 0.25);
        let diff: Vec<f32> = l.iter().zip(&n).map(|(a, b)| a - b).collect();
        assert!(rms(&diff) > 0.01, "{id}");
    }
}

#[test]
fn block_size_does_not_change_the_sound() {
    let sr = 48_000;
    let n = noise(sr as usize, 0.5, 31);
    for id in EXPECTED_KEYS {
        let mut a = make(id, sr, 1024);
        let mut b = make(id, sr, 1024);
        let (la, _) = run(a.as_mut(), sr, 1024, &n, &n, Some(120.0), 0.5);
        let (lb, _) = run(b.as_mut(), sr, 96, &n, &n, Some(120.0), 0.5);
        let worst = la
            .iter()
            .zip(&lb)
            .fold(0.0_f32, |m, (x, y)| m.max((x - y).abs()));
        assert!(worst < 2e-3, "{id}: {worst}");
    }
}

// --- reference slot host ---------------------------------------------------

fn slot_run(slot: &mut FxSlot, input: &[f32], enable: &[(usize, bool)]) -> Vec<f32> {
    let sr = 48_000;
    let mut out = input.to_vec();
    let mut r = input.to_vec();
    let mut pos = 0;
    let mut ev = enable.iter().peekable();
    while pos < out.len() {
        while let Some((at, on)) = ev.peek() {
            if *at <= pos {
                slot.set_enabled(*on);
                ev.next();
            } else {
                break;
            }
        }
        let end = (pos + 128).min(out.len());
        let c = ctx(sr, Some(120.0), Some(pos as f64 / 24_000.0), 0.5);
        let io = StereoBufferMut::new(&mut out[pos..end], &mut r[pos..end]).expect("len");
        assert_no_alloc::assert_no_alloc(|| slot.process(io, &c));
        pos = end;
    }
    out
}

#[test]
fn bypass_crossfade_has_no_discontinuity() {
    let sr = 48_000;
    let input = sine(3 * sr as usize, sr, 1000.0, 0.5);
    let max_input_step = 0.5 * std::f32::consts::TAU * 1000.0 / sr as f32;
    for id in [
        "delay",
        "echo",
        "reverb",
        "flanger",
        "phaser",
        "filter",
        "colour_filter",
        "pitch",
        "spiral",
    ] {
        let mut slot = FxSlot::new(BuiltinFactory.create(id).expect("creates"));
        slot.prepare(SampleRate(sr), 128);
        slot.set_mix(1.0);
        let out = slot_run(
            &mut slot,
            &input,
            &[(sr as usize / 2, true), (2 * sr as usize, false)],
        );
        let step = out
            .windows(2)
            .fold(0.0_f32, |m, w| m.max((w[1] - w[0]).abs()));
        assert!(
            step < 3.0 * max_input_step * peak(&out).max(0.5) / 0.5 + 0.02,
            "{id}: step {step}"
        );
    }
}

#[test]
fn slot_bypassed_is_the_dry_signal_and_tail_spills() {
    let sr = 48_000;
    let input = noise(3 * sr as usize, 0.5, 41);
    let mut slot = FxSlot::new(BuiltinFactory.create("delay").expect("creates"));
    slot.prepare(SampleRate(sr), 128);
    // Disabled from the start: output equals input exactly.
    let out = slot_run(&mut slot, &input, &[]);
    assert_eq!(out, input);
    assert!(!slot.is_active());

    // Enabled for 0.5 s then bypassed: the delay tail keeps sounding, then the slot idles.
    let mut slot = FxSlot::new(BuiltinFactory.create("delay").expect("creates"));
    slot.prepare(SampleRate(sr), 128);
    let mut input = noise(sr as usize / 2, 0.5, 43);
    input.extend(std::iter::repeat_n(0.0, 6 * sr as usize));
    let out = slot_run(&mut slot, &input, &[(0, true), (sr as usize / 4, false)]);
    // The input is silent after 0.5 s, so only a spilled tail can sound at 0.6..0.8 s.
    let spill = rms(&out[sr as usize * 6 / 10..sr as usize * 8 / 10]);
    assert!(spill > 0.01, "tail spilled: {spill}");
    assert!(
        !slot.is_active(),
        "tail decayed below -90 dBFS and the slot idled"
    );
}

#[test]
fn slot_compensates_latency_on_the_dry_path() {
    let sr = 48_000;
    let mut slot = FxSlot::new(BuiltinFactory.create("pitch").expect("creates"));
    slot.prepare(SampleRate(sr), 128);
    let mut input = vec![0.0; 4_000];
    input[10] = 1.0;
    let out = slot_run(&mut slot, &input, &[]);
    assert_eq!(out[10 + 960], 1.0);
    assert_eq!(out[10], 0.0);
}

// --- CPU -------------------------------------------------------------------

#[test]
fn cpu_per_block_table() {
    let sr = 48_000;
    let block = 512;
    let blocks = if cfg!(debug_assertions) { 200 } else { 4000 };
    let budget_us = block as f64 / f64::from(sr) * 1e6;
    let input = noise(block, 0.5, 99);
    println!("effect            us/block   % of {budget_us:.0} us budget");
    for id in EXPECTED_KEYS {
        let mut fx = make(id, sr, block);
        let mut l = input.clone();
        let mut r = input.clone();
        for b in 0..50 {
            l.copy_from_slice(&input);
            r.copy_from_slice(&input);
            let c = ctx(sr, Some(120.0), Some((b * block) as f64 / 24_000.0), 0.5);
            process_guarded(fx.as_mut(), &mut l, &mut r, &c);
        }
        let t0 = std::time::Instant::now();
        for b in 0..blocks {
            l.copy_from_slice(&input);
            r.copy_from_slice(&input);
            let c = ctx(
                sr,
                Some(120.0),
                Some(((b + 50) * block) as f64 / 24_000.0),
                0.5,
            );
            process_guarded(fx.as_mut(), &mut l, &mut r, &c);
        }
        let us = t0.elapsed().as_secs_f64() * 1e6 / blocks as f64;
        println!("{id:<16} {us:>9.1}   {:>6.2}", us / budget_us * 100.0);
        #[cfg(not(debug_assertions))]
        assert!(us < 0.2 * budget_us, "{id} uses {us} us per block");
    }
}
