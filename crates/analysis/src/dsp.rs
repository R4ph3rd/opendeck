//! Shared helpers: input validation, cancellation, mono access, FFT frames.

use std::sync::Arc;

use opendeck_contracts::analysis::{AnalysisError, AudioView, CancelFlag};
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlannerScalar};

/// Returns `Err(Cancelled)` once the flag is set.
pub(crate) fn check(cancel: &CancelFlag) -> Result<(), AnalysisError> {
    if cancel.is_cancelled() {
        Err(AnalysisError::Cancelled)
    } else {
        Ok(())
    }
}

/// Validates the view invariants and a minimum length in seconds.
pub(crate) fn validate(audio: &AudioView<'_>, min_seconds: f64) -> Result<(), AnalysisError> {
    if !audio.is_valid() {
        return Err(AnalysisError::InvalidInput(
            "audio view violates invariants".into(),
        ));
    }
    let seconds = audio.frames() as f64 / audio.sample_rate.as_f64();
    if seconds < min_seconds {
        return Err(AnalysisError::InvalidInput(format!(
            "audio shorter than {min_seconds} s"
        )));
    }
    Ok(())
}

/// Copies the mono mix `(L + R) / 2` of `audio[start..start + out.len()]`
/// into `out`, zero-padding past the end.
pub(crate) fn fill_mono(audio: &AudioView<'_>, start: usize, out: &mut [f32]) {
    let n = audio.frames();
    let avail = n.saturating_sub(start).min(out.len());
    match audio.channels {
        [m] => out[..avail].copy_from_slice(&m[start..start + avail]),
        [l, r] => {
            for (i, o) in out[..avail].iter_mut().enumerate() {
                *o = 0.5 * (l[start + i] + r[start + i]);
            }
        }
        _ => {}
    }
    out[avail..].fill(0.0);
}

/// Hann window of length `n` (periodic).
pub(crate) fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = std::f64::consts::TAU * i as f64 / n as f64;
            (0.5 - 0.5 * x.cos()) as f32
        })
        .collect()
}

/// Windowed forward FFT of mono frames. Uses the scalar rustfft planner so
/// the arithmetic is the same on every CPU (no SIMD dispatch), which keeps
/// results identical across platforms.
pub(crate) struct Stft {
    pub(crate) win: usize,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
    buf: Vec<Complex32>,
    scratch: Vec<Complex32>,
    mono: Vec<f32>,
}

impl Stft {
    pub(crate) fn new(win: usize) -> Self {
        let fft = FftPlannerScalar::<f32>::new().plan_fft_forward(win);
        let scratch = vec![Complex32::default(); fft.get_inplace_scratch_len()];
        Self {
            win,
            window: hann(win),
            fft,
            buf: vec![Complex32::default(); win],
            scratch,
            mono: vec![0.0; win],
        }
    }

    /// Transforms the frame starting at `start`; bins `0..=win/2` are
    /// available from [`Stft::bin`].
    pub(crate) fn run(&mut self, audio: &AudioView<'_>, start: usize) {
        fill_mono(audio, start, &mut self.mono);
        for ((b, &x), &w) in self.buf.iter_mut().zip(&self.mono).zip(&self.window) {
            *b = Complex32::new(x * w, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buf, &mut self.scratch);
    }

    /// Magnitude of bin `k` of the last frame, normalised by the window
    /// length (a full-scale sine reads about 0.25).
    #[inline]
    pub(crate) fn bin(&self, k: usize) -> f32 {
        self.buf[k].norm() / self.win as f32
    }
}

/// Median of a slice (lower middle for even lengths). Empty gives 0.
pub(crate) fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    values[(values.len() - 1) / 2]
}

/// Rounds to `decimals` decimal places so tiny cross-platform float noise
/// cannot change stored output.
pub(crate) fn round_to(v: f64, decimals: i32) -> f64 {
    let m = 10f64.powi(decimals);
    (v * m).round() / m
}
