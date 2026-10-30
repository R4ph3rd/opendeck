//! Deterministic synthetic audio for the labelled accuracy set. No recorded
//! audio is used anywhere (licence-clean by construction).
#![allow(dead_code, missing_docs)]

use std::f64::consts::PI;

pub const SR: u32 = 44_100;

/// Small xorshift PRNG so every platform generates identical audio.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Uniform in `[-1, 1)`.
    pub fn bipolar(&mut self) -> f64 {
        self.next_f64() * 2.0 - 1.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrumCase {
    pub bpm: f64,
    /// Time of the first downbeat, seconds.
    pub offset: f64,
    /// Off-beat delay as a fraction of a beat (0 = straight, 0.17 = triplet swing).
    pub swing: f64,
    /// Background white noise level in dB relative to full scale.
    pub noise_db: f64,
    /// Max random timing error of each hit, seconds.
    pub jitter: f64,
    pub seconds: f64,
}

fn add_kick(buf: &mut [f64], at: f64, amp: f64) {
    let start = (at * f64::from(SR)).round() as isize;
    let mut phase = 0.0;
    for i in 0..13_000_isize {
        let idx = start + i;
        if idx < 0 || idx as usize >= buf.len() {
            continue;
        }
        let t = i as f64 / f64::from(SR);
        let f = 45.0 + 115.0 * (-t / 0.03).exp();
        phase += 2.0 * PI * f / f64::from(SR);
        buf[idx as usize] += amp * phase.sin() * (-t / 0.11).exp();
    }
}

fn add_noise_burst(buf: &mut [f64], rng: &mut Rng, at: f64, amp: f64, len_s: f64) {
    let start = (at * f64::from(SR)).round() as isize;
    let n = (len_s * f64::from(SR)) as isize;
    for i in 0..n {
        let idx = start + i;
        if idx < 0 || idx as usize >= buf.len() {
            continue;
        }
        let t = i as f64 / f64::from(SR);
        buf[idx as usize] += amp * rng.bipolar() * (-t / (len_s * 0.35)).exp();
    }
}

/// Kick on every beat (louder on beat 1 of each 4/4 bar), snare on 2 and 4,
/// off-beat hats (swung), optional noise bed and timing jitter. Returns mono.
pub fn drums(case: DrumCase, seed: u64) -> Vec<f64> {
    let n = (case.seconds * f64::from(SR)) as usize;
    let mut buf = vec![0.0; n];
    let mut rng = Rng::new(seed);
    let beat = 60.0 / case.bpm;
    let mut k = 0_i64;
    loop {
        let t = case.offset + k as f64 * beat;
        if t > case.seconds {
            break;
        }
        let j = |rng: &mut Rng| rng.bipolar() * case.jitter;
        let pos = k.rem_euclid(4);
        let amp = if pos == 0 { 0.9 } else { 0.6 };
        add_kick(&mut buf, t + j(&mut rng), amp);
        if pos == 1 || pos == 3 {
            let at = t + j(&mut rng);
            add_noise_burst(&mut buf, &mut rng, at, 0.35, 0.09);
        }
        let hat = t + beat * (0.5 + case.swing) + j(&mut rng);
        add_noise_burst(&mut buf, &mut rng, hat, 0.12, 0.025);
        k += 1;
    }
    if case.noise_db > -200.0 {
        let a = 10f64.powf(case.noise_db / 20.0);
        for s in &mut buf {
            *s += a * rng.bipolar();
        }
    }
    buf
}

fn midi_hz(note: f64) -> f64 {
    440.0 * 2f64.powf((note - 69.0) / 12.0)
}

fn add_note(buf: &mut [f64], start_s: f64, len_s: f64, note: f64, amp: f64) {
    let f0 = midi_hz(note);
    let start = (start_s * f64::from(SR)) as usize;
    let n = (len_s * f64::from(SR)) as usize;
    for i in 0..n {
        let idx = start + i;
        if idx >= buf.len() {
            break;
        }
        let t = i as f64 / f64::from(SR);
        let env = (t / 0.05).min(1.0) * ((len_s - t) / 0.05).clamp(0.0, 1.0);
        let mut s = 0.0;
        for h in 1..=6 {
            let f = f0 * f64::from(h);
            if f < 8000.0 {
                s += (2.0 * PI * f * t).sin() / f64::from(h);
            }
        }
        buf[idx] += amp * env * s;
    }
}

/// Pad chord progression in `tonic` (0 = C) and mode, detuned by `cents`.
/// Major: I IV V I. Minor: i iv V i. Each chord lasts 3 s, repeated to fill
/// `seconds`.
pub fn pads(tonic: u8, minor: bool, cents: f64, seconds: f64) -> Vec<f64> {
    let n = (seconds * f64::from(SR)) as usize;
    let mut buf = vec![0.0; n];
    let base = 48.0 + f64::from(tonic) + cents / 100.0;
    let third = if minor { 3.0 } else { 4.0 };
    // (root offset, third offset) per chord; V always major.
    let chords: [(f64, f64); 4] = [(0.0, third), (5.0, third), (7.0, 4.0), (0.0, third)];
    let mut t = 0.0;
    let mut i = 0;
    while t < seconds {
        let (r, th) = chords[i % 4];
        let root = base + r;
        add_note(&mut buf, t, 3.0, root - 12.0, 0.18);
        add_note(&mut buf, t, 3.0, root + 12.0, 0.12);
        add_note(&mut buf, t, 3.0, root + th + 12.0, 0.12);
        add_note(&mut buf, t, 3.0, root + 7.0 + 12.0, 0.12);
        t += 3.0;
        i += 1;
    }
    buf
}

/// Mono to planar stereo `f32` (identical channels).
pub fn to_stereo(mono: &[f64]) -> [Vec<f32>; 2] {
    let v: Vec<f32> = mono.iter().map(|&x| x as f32).collect();
    [v.clone(), v]
}

/// The labelled tempo set: (case, seed). 8 tempos x 3 conditions + 4 fractional.
pub fn tempo_set() -> Vec<(DrumCase, u64)> {
    let mut v = Vec::new();
    let mut seed = 1;
    for &bpm in &[90.0, 100.0, 120.0, 124.0, 128.0, 140.0, 150.0, 174.0] {
        for (offset, swing, noise_db, jitter) in [
            (0.0, 0.0, -300.0, 0.0),
            (0.137, 0.15, -30.0, 0.002),
            (0.31, 0.22, -20.0, 0.003),
        ] {
            v.push((
                DrumCase {
                    bpm,
                    offset,
                    swing,
                    noise_db,
                    jitter,
                    seconds: 40.0,
                },
                seed,
            ));
            seed += 1;
        }
    }
    for &bpm in &[98.6, 123.45, 132.8, 87.3] {
        v.push((
            DrumCase {
                bpm,
                offset: 0.21,
                swing: 0.1,
                noise_db: -30.0,
                jitter: 0.002,
                seconds: 40.0,
            },
            seed,
        ));
        seed += 1;
    }
    v
}

/// The labelled key set: (tonic, minor, detune cents).
pub fn key_set() -> Vec<(u8, bool, f64)> {
    let mut v = Vec::new();
    for t in 0..12u8 {
        let cents = [0.0, 20.0, -25.0][usize::from(t) % 3];
        v.push((t, false, cents));
        v.push((t, true, -cents));
    }
    v
}
