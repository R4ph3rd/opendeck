//! Offline render tests for the mixer: EQ, filter, faders, crossfader, cue
//! and headphone bus, meters. Every render runs under `assert_no_alloc`.

use opendeck_contracts::mixer::{CrossfaderAssign, CrossfaderCurve, EqBand};
use opendeck_contracts::{EngineCommand, Gain};

use crate::tests_support::*;
use crate::{EQ_LOW_MID_HZ, EQ_MID_HIGH_HZ, TrackBuffer};

/// Geometric centres of the three EQ bands (20 Hz and 20 kHz are the outer edges).
const LOW_CENTRE: f64 = 63.2;
const MID_CENTRE: f64 = 707.1;
const HIGH_CENTRE: f64 = 7_071.0;

/// Master RMS (left channel, after settling) of a sine through a rig set up by `setup`.
fn level(freq: f64, setup: impl FnOnce(&mut Rig)) -> f64 {
    let mut rig = Rig::new(2);
    rig.load(0, sine(freq, 4.0), None);
    setup(&mut rig);
    rig.play(0, true);
    rig.step(SR as usize / 2);
    let out = rig.step(SR as usize);
    rms(&left(&out))
}

fn gain_change_db(freq: f64, setup: impl FnOnce(&mut Rig)) -> f64 {
    let reference = level(freq, |_| {});
    db(level(freq, setup) / reference)
}

fn eq(rig: &mut Rig, band: EqBand, gain: f32) {
    rig.send(EngineCommand::SetEq {
        deck: deck(0),
        band,
        gain: Gain(gain),
    });
}

fn right(stereo: &[f32]) -> Vec<f32> {
    stereo.chunks_exact(2).map(|f| f[1]).collect()
}

// ----- EQ -----

#[test]
fn eq_kill_attenuates_each_band_by_more_than_40_db_at_its_centre() {
    for (band, centre) in [
        (EqBand::Low, LOW_CENTRE),
        (EqBand::Mid, MID_CENTRE),
        (EqBand::High, HIGH_CENTRE),
    ] {
        let change = gain_change_db(centre, |rig| eq(rig, band, 0.0));
        println!("kill {band:?} at {centre:.0} Hz: {change:.1} dB");
        assert!(change < -40.0, "{band:?} kill only {change:.1} dB");
    }
}

#[test]
fn eq_kill_leaves_the_other_bands_alone() {
    for (band, others) in [
        (EqBand::Low, [MID_CENTRE, HIGH_CENTRE]),
        (EqBand::Mid, [LOW_CENTRE, HIGH_CENTRE]),
        (EqBand::High, [LOW_CENTRE, MID_CENTRE]),
    ] {
        for freq in others {
            let change = gain_change_db(freq, |rig| eq(rig, band, 0.0));
            assert!(
                change.abs() < 0.6,
                "kill {band:?} moved {freq} Hz by {change:.2} dB"
            );
        }
    }
}

#[test]
fn eq_is_flat_when_engaged_with_neutral_gains_and_boosts_six_db() {
    for freq in [
        40.0,
        LOW_CENTRE,
        EQ_LOW_MID_HZ,
        MID_CENTRE,
        EQ_MID_HIGH_HZ,
        HIGH_CENTRE,
        14_000.0,
    ] {
        // One band a hair off unity keeps the filter path engaged.
        let change = gain_change_db(freq, |rig| eq(rig, EqBand::High, 1.000_01));
        assert!(change.abs() < 0.1, "{freq} Hz: {change:.3} dB");
    }
    let boost = gain_change_db(MID_CENTRE, |rig| eq(rig, EqBand::Mid, 4.0));
    println!("mid boost, asked for 4.0 (clamped to +6 dB): {boost:.2} dB");
    assert!((boost - 6.0).abs() < 0.4, "{boost}");
    let cut = gain_change_db(LOW_CENTRE, |rig| eq(rig, EqBand::Low, 0.5));
    assert!((cut + 6.02).abs() < 0.4, "{cut}");
}

#[test]
fn eq_engages_and_releases_without_a_click() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(100.0, 10.0), None);
    rig.play(0, true);
    rig.step(SR as usize / 4);
    eq(&mut rig, EqBand::Low, 0.0);
    let killed = rig.step(SR as usize / 4);
    eq(&mut rig, EqBand::Low, 1.0);
    let back = rig.step(SR as usize / 2);
    // A 100 Hz sine at 0.5 steps by 0.0065 per sample.
    for (name, out) in [("kill", &killed), ("release", &back)] {
        let step = max_step(&out[200..]);
        println!("{name}: max step {step:.4}");
        assert!(step < 0.03, "{name} clicked: {step}");
    }
    assert!(rms(&left(&killed[killed.len() / 2..])) < 0.01);
    assert!(rms(&left(&back[back.len() / 2..])) > 0.3);
}

#[test]
fn eq_at_unity_is_bit_exact() {
    let samples = sine_samples(440.0, 1.0, 0.5);
    let mut rig = Rig::new(2);
    rig.load(
        0,
        TrackBuffer::from_vec(samples.clone()).expect("track"),
        None,
    );
    eq(&mut rig, EqBand::Mid, 1.0);
    rig.play(0, true);
    let out = rig.step(SR as usize / 2);
    assert_eq!(&out[1000..20_000], &samples[1000..20_000]);
}

#[test]
fn eq_gains_are_clamped() {
    let loud = gain_change_db(MID_CENTRE, |rig| eq(rig, EqBand::Mid, 100.0));
    assert!(loud < 6.5, "{loud}");
    let nan = gain_change_db(MID_CENTRE, |rig| eq(rig, EqBand::Mid, f32::NAN));
    assert!(nan.abs() < 1e-6, "NaN is ignored: {nan}");
}

// ----- Filter -----

fn filter(rig: &mut Rig, amount: f32) {
    rig.send(EngineCommand::SetFilter {
        deck: deck(0),
        amount,
    });
}

#[test]
fn filter_low_pass_on_the_left_high_pass_on_the_right() {
    let low = gain_change_db(200.0, |rig| filter(rig, -0.5));
    let high = gain_change_db(8_000.0, |rig| filter(rig, -0.5));
    println!("LP -0.5: 200 Hz {low:.1} dB, 8 kHz {high:.1} dB");
    assert!(low.abs() < 1.5, "{low}");
    assert!(high < -20.0, "{high}");

    let low = gain_change_db(100.0, |rig| filter(rig, 0.5));
    let high = gain_change_db(5_000.0, |rig| filter(rig, 0.5));
    println!("HP +0.5: 100 Hz {low:.1} dB, 5 kHz {high:.1} dB");
    assert!(low < -9.0, "{low}");
    assert!(high.abs() < 1.5, "{high}");

    let a = gain_change_db(1_000.0, |rig| filter(rig, -0.3));
    let b = gain_change_db(1_000.0, |rig| filter(rig, -0.9));
    assert!(b < a - 10.0, "{a} {b}");
    let a = gain_change_db(1_000.0, |rig| filter(rig, 0.3));
    let b = gain_change_db(1_000.0, |rig| filter(rig, 0.9));
    assert!(b < a - 10.0, "{a} {b}");
}

#[test]
fn filter_has_a_mild_resonance_and_a_dead_zone() {
    let at_cutoff = gain_change_db(100.0, |rig| filter(rig, -1.0));
    println!("LP at its cutoff: {at_cutoff:.2} dB");
    assert!(at_cutoff > -1.0 && at_cutoff < 4.0, "{at_cutoff}");

    let samples = sine_samples(1000.0, 1.0, 0.5);
    for amount in [0.0, 0.015, -0.019] {
        let mut rig = Rig::new(2);
        rig.load(
            0,
            TrackBuffer::from_vec(samples.clone()).expect("track"),
            None,
        );
        filter(&mut rig, amount);
        rig.play(0, true);
        let out = rig.step(SR as usize / 2);
        assert_eq!(
            &out[1000..20_000],
            &samples[1000..20_000],
            "amount {amount}"
        );
    }
    let near = gain_change_db(1_000.0, |rig| filter(rig, 0.03));
    assert!(near.abs() < 0.5, "{near}");
}

#[test]
fn filter_sweeps_without_clicks() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(300.0, 10.0), None);
    rig.play(0, true);
    rig.step(SR as usize / 4);
    let mut out = Vec::new();
    for i in 0..200 {
        let amount = -1.0 + 2.0 * i as f32 / 199.0;
        filter(&mut rig, amount);
        out.extend(rig.step(BLOCK));
    }
    assert!(out.iter().all(|s| s.is_finite() && s.abs() < 1.5));
    let step = max_step(&out[4000..]);
    println!("filter sweep: max sample step {step:.3} (sine alone 0.020)");
    assert!(step < 0.2, "{step}");
}

// ----- Faders, trim -----

#[test]
fn channel_fader_changes_do_not_zipper() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(100.0, 10.0), None);
    rig.play(0, true);
    rig.step(SR as usize / 4);
    rig.send(EngineCommand::SetChannelFader {
        deck: deck(0),
        position: 0.0,
    });
    let out = rig.step(2 * BLOCK);
    let step = max_step(&out[100..]);
    println!("fader slam: max step {step:.4}");
    assert!(step < 0.01 + 0.5 / BLOCK as f32 * 1.2, "{step}");
    assert!(peak(&out[2 * BLOCK..]) < 1e-6);
}

#[test]
fn trim_is_clamped_to_its_maximum() {
    let base = level(1000.0, |_| {});
    let loud = level(1000.0, |rig| {
        rig.send(EngineCommand::SetTrim {
            deck: deck(0),
            gain: Gain(100.0),
        });
    });
    let ratio_db = db(loud / base);
    assert!((ratio_db - 12.0).abs() < 0.1, "{ratio_db}");
}

// ----- Crossfader -----

fn crossfader_gain(curve: CrossfaderCurve, assign: CrossfaderAssign, position: f32) -> f64 {
    let reference = level(1000.0, |_| {});
    let value = level(1000.0, |rig| {
        rig.send(EngineCommand::SetCrossfaderCurve { curve });
        rig.send(EngineCommand::SetCrossfaderAssign {
            deck: deck(0),
            assign,
        });
        rig.send(EngineCommand::SetCrossfader { position });
    });
    value / reference
}

#[test]
fn crossfader_curves_hit_their_endpoints_and_centre() {
    use CrossfaderAssign::{A, B, Thru};
    use CrossfaderCurve::{ConstantPower, Linear, Sharp};
    let near = |got: f64, want: f64| (got - want).abs() < 0.01;
    for (curve, centre) in [
        (ConstantPower, std::f64::consts::FRAC_1_SQRT_2),
        (Linear, 0.5),
        (Sharp, 1.0),
    ] {
        assert!(
            near(crossfader_gain(curve, A, -1.0), 1.0),
            "{curve:?} A at A"
        );
        assert!(
            near(crossfader_gain(curve, A, 1.0), 0.0),
            "{curve:?} A at B"
        );
        assert!(
            near(crossfader_gain(curve, B, 1.0), 1.0),
            "{curve:?} B at B"
        );
        assert!(
            near(crossfader_gain(curve, B, -1.0), 0.0),
            "{curve:?} B at A"
        );
        assert!(
            near(crossfader_gain(curve, A, 0.0), centre),
            "{curve:?} A centre"
        );
        assert!(
            near(crossfader_gain(curve, B, 0.0), centre),
            "{curve:?} B centre"
        );
        assert!(
            near(crossfader_gain(curve, Thru, 1.0), 1.0),
            "{curve:?} thru"
        );
        assert!(
            near(crossfader_gain(curve, Thru, -1.0), 1.0),
            "{curve:?} thru"
        );
    }
    assert!(near(crossfader_gain(Sharp, A, 0.9), 1.0));
    assert!(near(crossfader_gain(Sharp, A, 0.95), 0.5));
}

#[test]
fn crossfader_sums_two_decks_with_their_curve_gains() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(1000.0, 4.0), None);
    rig.load(1, sine(1000.0, 4.0), None);
    rig.send(EngineCommand::SetCrossfaderAssign {
        deck: deck(0),
        assign: CrossfaderAssign::A,
    });
    rig.send(EngineCommand::SetCrossfaderAssign {
        deck: deck(1),
        assign: CrossfaderAssign::B,
    });
    rig.play(0, true);
    rig.play(1, true);
    rig.step(SR as usize / 2);
    // In-phase sines add: constant power at the centre gives 2 * 0.7071.
    let one = level(1000.0, |_| {});
    let out = rig.step(SR as usize);
    let sum = rms(&left(&out)) / one;
    assert!((sum - std::f64::consts::SQRT_2).abs() < 0.02, "{sum}");
}

// ----- Cue and headphones -----

/// A track with a 1 kHz sine on the left and silence on the right.
fn left_only(amp: f32) -> TrackBuffer {
    let mut samples = sine_samples(1000.0, 4.0, amp);
    for frame in samples.chunks_exact_mut(2) {
        frame[1] = 0.0;
    }
    TrackBuffer::from_vec(samples).expect("track")
}

fn render_both(rig: &mut Rig, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut master = vec![0.0; frames * 2];
    let mut phones = vec![0.0; frames * 2];
    rig.r.render_both(&mut master, &mut phones);
    (master, phones)
}

#[test]
fn split_cue_puts_cue_mono_on_the_left_and_master_mono_on_the_right() {
    let mut rig = Rig::new(2);
    rig.load(0, left_only(0.5), None);
    rig.send(EngineCommand::SetPfl {
        deck: deck(0),
        on: true,
    });
    rig.send(EngineCommand::SetChannelFader {
        deck: deck(0),
        position: 0.5,
    });
    rig.send(EngineCommand::SetSplitCue { on: true });
    rig.play(0, true);
    render_both(&mut rig, SR as usize / 4);
    let (master, phones) = render_both(&mut rig, SR as usize / 2);

    assert!((f64::from(peak(&left(&master))) - 0.25).abs() < 0.01);
    assert!(peak(&right(&master)) < 1e-6);
    let cue_left = peak(&left(&phones));
    let master_right = peak(&right(&phones));
    println!("split cue: left ear {cue_left:.3}, right ear {master_right:.3}");
    assert!((cue_left - 0.25).abs() < 0.01, "{cue_left}");
    assert!((master_right - 0.125).abs() < 0.01, "{master_right}");
}

#[test]
fn headphone_mix_blends_cue_and_master() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(1000.0, 4.0), None);
    rig.load(1, sine(1000.0, 4.0), None);
    // Deck 0 is on air, deck 1 is only in the headphones.
    rig.send(EngineCommand::SetPfl {
        deck: deck(1),
        on: true,
    });
    rig.send(EngineCommand::SetChannelFader {
        deck: deck(1),
        position: 0.0,
    });
    rig.play(0, true);
    rig.play(1, true);
    let level_at = |rig: &mut Rig, mix: f32| {
        rig.send(EngineCommand::SetHeadphoneMix { mix });
        render_both(rig, SR as usize / 4);
        let (_, phones) = render_both(rig, SR as usize / 2);
        rms(&left(&phones))
    };
    let full = 0.5 / std::f64::consts::SQRT_2;
    let cue_only = level_at(&mut rig, 0.0);
    let master_only = level_at(&mut rig, 1.0);
    let both = level_at(&mut rig, 0.5);
    println!("headphones: cue {cue_only:.3}, master {master_only:.3}, blend {both:.3}");
    assert!((cue_only - full).abs() < 0.01, "cue only {cue_only}");
    assert!(
        (master_only - full).abs() < 0.01,
        "master only {master_only}"
    );
    assert!(
        (both - 2.0 * full * std::f64::consts::FRAC_1_SQRT_2).abs() < 0.01,
        "{both}"
    );

    rig.send(EngineCommand::SetPfl {
        deck: deck(1),
        on: false,
    });
    let silent = level_at(&mut rig, 0.0);
    assert!(silent < 1e-6, "{silent}");
}

#[test]
fn headphone_level_scales_the_bus() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(1000.0, 4.0), None);
    rig.send(EngineCommand::SetPfl {
        deck: deck(0),
        on: true,
    });
    rig.play(0, true);
    render_both(&mut rig, SR as usize / 4);
    let (_, full) = render_both(&mut rig, SR as usize / 4);
    rig.send(EngineCommand::SetHeadphoneLevel { gain: Gain(0.25) });
    render_both(&mut rig, BLOCK);
    let (_, quiet) = render_both(&mut rig, SR as usize / 4);
    let ratio = rms(&quiet) / rms(&full);
    assert!((ratio - 0.25).abs() < 0.01, "{ratio}");
}

#[test]
fn master_only_render_matches_the_dual_render() {
    let make = || {
        let mut rig = Rig::new(2);
        rig.load(0, sine(1000.0, 4.0), None);
        eq(&mut rig, EqBand::Low, 0.5);
        rig.play(0, true);
        rig
    };
    let mut a = make();
    let mut b = make();
    let plain = a.step(SR as usize / 2);
    let (dual, _) = render_both(&mut b, SR as usize / 2);
    assert_eq!(plain, dual);
}

// ----- Meters, decks -----

#[test]
fn meters_report_channel_master_and_headphones() {
    let mut rig = Rig::new(2);
    rig.load(0, sine(1000.0, 4.0), None);
    rig.send(EngineCommand::SetChannelFader {
        deck: deck(0),
        position: 0.5,
    });
    rig.send(EngineCommand::SetPfl {
        deck: deck(0),
        on: true,
    });
    rig.play(0, true);
    render_both(&mut rig, SR as usize / 2);
    let t = rig.telemetry();
    let channel = t.decks[0].meter;
    println!(
        "meters: channel {channel:?}, master {:?}, headphones {:?}",
        t.master, t.headphones
    );
    assert!(
        (channel.peak_l - 0.25).abs() < 0.02,
        "post-fader channel peak"
    );
    assert!((f64::from(channel.rms_l) - 0.25 / std::f64::consts::SQRT_2).abs() < 0.02);
    assert!((t.master.peak_l - 0.25).abs() < 0.02);
    assert!(
        (t.headphones.peak_l - 0.5).abs() < 0.03,
        "cue is pre-fader: {}",
        t.headphones.peak_l
    );
    assert_eq!(t.decks[1].meter.peak_l, 0.0);
}

#[test]
fn four_decks_mix_into_one_master() {
    let mut rig = Rig::new(4);
    for (i, freq) in [200.0, 330.0, 550.0, 910.0].into_iter().enumerate() {
        rig.load(i as u8, sine(freq, 4.0), None);
        rig.play(i as u8, true);
    }
    rig.step(SR as usize / 2);
    let out = rig.step(SR as usize);
    let total = rms(&left(&out));
    assert!((total - 0.707).abs() < 0.03, "{total}");
    let t = rig.telemetry();
    for deck in &t.decks {
        assert!(deck.loaded && deck.playing);
    }
}

// ----- CPU -----

#[test]
fn two_key_locked_decks_with_eq_and_filter_fit_the_budget() {
    let mut rig = Rig::new(2);
    for i in 0..2u8 {
        rig.load(
            i,
            sine(300.0 + 200.0 * f64::from(i), 70.0),
            Some(grid(120.0)),
        );
        rig.send(EngineCommand::SetKeyLock {
            deck: deck(i),
            on: true,
        });
        rig.send(EngineCommand::SetTempoFader {
            deck: deck(i),
            position: 0.6,
        });
        rig.send(EngineCommand::SetEq {
            deck: deck(i),
            band: EqBand::Mid,
            gain: Gain(0.5),
        });
        rig.send(EngineCommand::SetFilter {
            deck: deck(i),
            amount: -0.4,
        });
        rig.send(EngineCommand::SetPfl {
            deck: deck(i),
            on: true,
        });
        rig.play(i, true);
    }
    rig.step(SR as usize);
    let before = rig.r.stats();
    let mut out = vec![0.0; 60 * SR as usize * 2];
    rig.r.render_into(&mut out);
    let stats = rig.r.stats();
    let blocks = stats.blocks - before.blocks;
    let total = stats.total - before.total;
    let mean = total / blocks as u32;
    let budget = BLOCK as f64 / f64::from(SR);
    let fraction = total.as_secs_f64() / ((stats.frames - before.frames) as f64 / f64::from(SR));
    println!(
        "2 decks, key lock + EQ + filter + PFL: mean {mean:?} per {BLOCK}-frame block (budget {:.2} ms), worst {:?}, {:.2}% of realtime",
        budget * 1000.0,
        stats.worst_block,
        fraction * 100.0
    );
    assert!(out.iter().all(|s| s.is_finite()));
    if !cfg!(debug_assertions) {
        assert!(fraction < 0.25, "too slow: {fraction}");
    }
}
