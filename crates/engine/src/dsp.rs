//! Small filter building blocks for the mixer: biquad cascades (Linkwitz-Riley
//! crossovers) and a state-variable filter. Allocation free; state is `f64`
//! so low crossover frequencies stay accurate.

use std::f64::consts::PI;

/// Direct form II transposed biquad.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

/// Which side of the cutoff a filter passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pass {
    Low,
    High,
}

/// Which end of the spectrum a shelf filter boosts or cuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shelf {
    Low,
    High,
}

impl Biquad {
    /// RBJ cookbook second-order low or high pass.
    pub(crate) fn new(pass: Pass, fc: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * (fc / sample_rate).clamp(1e-5, 0.49);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = match pass {
            Pass::Low => ((1.0 - cos) * 0.5, 1.0 - cos, (1.0 - cos) * 0.5),
            Pass::High => ((1.0 + cos) * 0.5, -(1.0 + cos), (1.0 + cos) * 0.5),
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// RBJ cookbook low or high shelf with shelf slope 1, `gain_db` boost or
    /// cut of the shelf region (`0` is flat).
    pub(crate) fn shelf(kind: Shelf, fc: f64, gain_db: f64, sample_rate: f64) -> Self {
        let a = 10.0_f64.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * (fc / sample_rate).clamp(1e-5, 0.49);
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / std::f64::consts::SQRT_2;
        let beta = 2.0 * a.sqrt() * alpha;
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Shelf::Low => (
                a * ((a + 1.0) - (a - 1.0) * cos + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                a * ((a + 1.0) - (a - 1.0) * cos - beta),
                (a + 1.0) + (a - 1.0) * cos + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                (a + 1.0) + (a - 1.0) * cos - beta,
            ),
            Shelf::High => (
                a * ((a + 1.0) + (a - 1.0) * cos + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                a * ((a + 1.0) + (a - 1.0) * cos - beta),
                (a + 1.0) - (a - 1.0) * cos + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * cos),
                (a + 1.0) - (a - 1.0) * cos - beta,
            ),
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// Takes the coefficients of `other` and keeps this filter's state, so a
    /// retune does not click.
    pub(crate) fn retune(&mut self, other: &Biquad) {
        self.b0 = other.b0;
        self.b1 = other.b1;
        self.b2 = other.b2;
        self.a1 = other.a1;
        self.a2 = other.a2;
    }

    #[inline]
    pub(crate) fn tick(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Biquads per Linkwitz-Riley filter.
pub(crate) const LR_SECTIONS: usize = 4;

/// Q of each biquad of a Linkwitz-Riley 8 filter: a 4th order Butterworth
/// (Q 0.5412 and 1.3066) applied twice.
const LR8_Q: [f64; LR_SECTIONS] = [0.541_196_1, 1.306_563, 0.541_196_1, 1.306_563];

/// Linkwitz-Riley 8th order low or high pass (48 dB per octave), one channel.
/// Low and high outputs at the same cutoff sum to an all-pass.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Lr {
    s: [Biquad; LR_SECTIONS],
}

impl Lr {
    pub(crate) fn new(pass: Pass, fc: f64, sample_rate: f64) -> Self {
        Self {
            s: LR8_Q.map(|q| Biquad::new(pass, fc, q, sample_rate)),
        }
    }

    #[inline]
    pub(crate) fn tick(&mut self, x: f64) -> f64 {
        self.s.iter_mut().fold(x, |acc, b| b.tick(acc))
    }
}

/// Zavalishin / Simper topology-preserving state-variable filter. Stable under
/// per-sample coefficient changes.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Svf {
    ic1: f64,
    ic2: f64,
}

/// Coefficients of an [`Svf`] for one cutoff and resonance.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SvfCoeffs {
    a1: f64,
    a2: f64,
    a3: f64,
    k: f64,
}

impl SvfCoeffs {
    pub(crate) fn new(fc: f64, q: f64, sample_rate: f64) -> Self {
        let g = (PI * (fc / sample_rate).clamp(1e-5, 0.49)).tan();
        let k = 1.0 / q.max(0.1);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self { a1, a2, a3, k }
    }
}

impl Svf {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns `(low pass, high pass)` for one input sample.
    #[inline]
    pub(crate) fn tick(&mut self, x: f64, c: &SvfCoeffs) -> (f64, f64) {
        let v3 = x - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        (v2, x - c.k * v1 - v2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    /// Steady-state gain of `process` for a sine at `freq`.
    fn gain_db(freq: f64, mut process: impl FnMut(f64) -> f64) -> f64 {
        let n = 48_000;
        let (mut sum_in, mut sum_out) = (0.0, 0.0);
        for i in 0..n {
            let x = (2.0 * PI * freq * i as f64 / SR).sin();
            let y = process(x);
            if i > n / 2 {
                sum_in += x * x;
                sum_out += y * y;
            }
        }
        10.0 * (sum_out / sum_in).log10()
    }

    #[test]
    fn lr8_is_minus_six_db_at_the_crossover_and_steep() {
        let mut lp = Lr::new(Pass::Low, 1000.0, SR);
        let at_fc = gain_db(1000.0, |x| lp.tick(x));
        assert!((at_fc + 6.02).abs() < 0.1, "{at_fc}");
        let mut lp = Lr::new(Pass::Low, 1000.0, SR);
        let two_octaves = gain_db(4000.0, |x| lp.tick(x));
        assert!(two_octaves < -90.0, "{two_octaves}");
    }

    #[test]
    fn lr8_low_plus_high_is_flat_in_magnitude() {
        for freq in [200.0, 1000.0, 5000.0] {
            let mut lp = Lr::new(Pass::Low, 1000.0, SR);
            let mut hp = Lr::new(Pass::High, 1000.0, SR);
            let g = gain_db(freq, |x| lp.tick(x) + hp.tick(x));
            assert!(g.abs() < 0.05, "{freq} Hz: {g} dB");
        }
    }

    #[test]
    fn shelves_boost_and_cut_their_own_end() {
        let mut low = Biquad::shelf(Shelf::Low, 200.0, 6.0, SR);
        let at_50 = gain_db(50.0, |x| low.tick(x));
        assert!((at_50 - 6.0).abs() < 0.3, "{at_50}");
        let mut low = Biquad::shelf(Shelf::Low, 200.0, 6.0, SR);
        let at_5k = gain_db(5000.0, |x| low.tick(x));
        assert!(at_5k.abs() < 0.3, "{at_5k}");
        let mut high = Biquad::shelf(Shelf::High, 4000.0, -12.0, SR);
        let at_12k = gain_db(12_000.0, |x| high.tick(x));
        assert!((at_12k + 12.0).abs() < 0.6, "{at_12k}");
        let mut high = Biquad::shelf(Shelf::High, 4000.0, -12.0, SR);
        let at_200 = gain_db(200.0, |x| high.tick(x));
        assert!(at_200.abs() < 0.3, "{at_200}");
        let mut flat = Biquad::shelf(Shelf::High, 4000.0, 0.0, SR);
        let g = gain_db(1000.0, |x| flat.tick(x));
        assert!(g.abs() < 1e-6, "{g}");
    }

    #[test]
    fn svf_low_pass_and_high_pass_split_the_spectrum() {
        let c = SvfCoeffs::new(1000.0, 0.707, SR);
        let mut f = Svf::default();
        let at_fc = gain_db(1000.0, |x| f.tick(x, &c).0);
        assert!((at_fc + 3.0).abs() < 0.3, "{at_fc}");
        let mut f = Svf::default();
        let high = gain_db(8000.0, |x| f.tick(x, &c).1);
        assert!(high.abs() < 0.5, "{high}");
        let mut f = Svf::default();
        let low_stop = gain_db(8000.0, |x| f.tick(x, &c).0);
        assert!(low_stop < -30.0, "{low_stop}");
    }
}
