//! Peak and RMS metering with peak hold and decay. Allocation free.

use opendeck_contracts::telemetry::StereoMeter;

/// Peak multiplier applied once per published snapshot (~60 Hz). 0.85 per
/// snapshot falls about 60 dB in a second.
const PEAK_DECAY_PER_SNAPSHOT: f32 = 0.85;

/// Accumulates levels between telemetry snapshots.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MeterAcc {
    peak_l: f32,
    peak_r: f32,
    sum_sq_l: f64,
    sum_sq_r: f64,
    frames: u64,
    held_l: f32,
    held_r: f32,
}

impl MeterAcc {
    /// Adds a block of interleaved stereo samples.
    pub(crate) fn add(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(2) {
            let (l, r) = (frame[0], frame[1]);
            self.peak_l = self.peak_l.max(l.abs());
            self.peak_r = self.peak_r.max(r.abs());
            self.sum_sq_l += f64::from(l) * f64::from(l);
            self.sum_sq_r += f64::from(r) * f64::from(r);
        }
        self.frames = self.frames.saturating_add((interleaved.len() / 2) as u64);
    }

    /// Reads the meter for one snapshot and starts a new accumulation window.
    /// Peaks hold the larger of the new peak and the decayed previous one.
    pub(crate) fn take(&mut self) -> StereoMeter {
        self.held_l = self.peak_l.max(self.held_l * PEAK_DECAY_PER_SNAPSHOT);
        self.held_r = self.peak_r.max(self.held_r * PEAK_DECAY_PER_SNAPSHOT);
        let n = self.frames.max(1) as f64;
        let meter = StereoMeter {
            peak_l: self.held_l,
            peak_r: self.held_r,
            rms_l: (self.sum_sq_l / n).sqrt() as f32,
            rms_r: (self.sum_sq_r / n).sqrt() as f32,
        };
        self.peak_l = 0.0;
        self.peak_r = 0.0;
        self.sum_sq_l = 0.0;
        self.sum_sq_r = 0.0;
        self.frames = 0;
        meter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_holds_and_decays_rms_resets() {
        let mut m = MeterAcc::default();
        m.add(&[0.5, -1.0, -0.5, 1.0]);
        let a = m.take();
        assert_eq!(a.peak_l, 0.5);
        assert_eq!(a.peak_r, 1.0);
        assert!((a.rms_l - 0.5).abs() < 1e-6);
        let b = m.take();
        assert!(b.peak_r < 1.0 && b.peak_r > 0.8);
        assert_eq!(b.rms_r, 0.0);
    }
}
