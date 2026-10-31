//! Test tone: a short sine on one output bus, for checking which device pair
//! is master, headphones or booth while the engine owns the device.
//!
//! The control thread stores a request in an atomic
//! ([`EngineHandle::play_test_tone`](crate::EngineHandle::play_test_tone));
//! the audio thread takes it at the start of a block and adds the tone to the
//! chosen bus after the mixer, so meters, the booth copy and the other buses
//! are untouched. A new request restarts the tone. Real-time safe: one atomic
//! swap per block, no allocation.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Tone frequency in Hz (A4).
pub const TEST_TONE_HZ: f32 = 440.0;
/// Tone length in milliseconds.
pub const TEST_TONE_MS: u32 = 900;
/// Linear peak level; about -14 dBFS, loud enough to hear and safe on
/// headphones (product choice).
pub const TEST_TONE_LEVEL: f32 = 0.2;
/// Fade in and out, in milliseconds, so the tone does not click.
const FADE_MS: f32 = 20.0;

/// Output bus that carries the test tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToneOutput {
    /// Master pair.
    Master,
    /// Headphone pair.
    Headphones,
    /// Booth pair.
    Booth,
}

impl ToneOutput {
    fn code(self) -> u32 {
        match self {
            ToneOutput::Master => 1,
            ToneOutput::Headphones => 2,
            ToneOutput::Booth => 3,
        }
    }

    fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(ToneOutput::Master),
            2 => Some(ToneOutput::Headphones),
            3 => Some(ToneOutput::Booth),
            _ => None,
        }
    }
}

/// Request slot shared by the handle (writer) and the core (reader).
pub(crate) fn tone_request() -> Arc<AtomicU32> {
    Arc::new(AtomicU32::new(0))
}

/// Asks the audio thread to play the tone on `output`.
pub(crate) fn request(slot: &AtomicU32, output: ToneOutput) {
    slot.store(output.code(), Ordering::Release);
}

/// Audio-side tone generator.
pub(crate) struct ToneRt {
    request: Arc<AtomicU32>,
    output: Option<ToneOutput>,
    phase: f32,
    step: f32,
    fade: f32,
    elapsed: u32,
    total: u32,
}

impl ToneRt {
    pub(crate) fn new(request: Arc<AtomicU32>, sample_rate: u32) -> Self {
        let rate = sample_rate.max(1) as f32;
        Self {
            request,
            output: None,
            phase: 0.0,
            step: TEST_TONE_HZ / rate,
            fade: (FADE_MS / 1000.0 * rate).max(1.0),
            elapsed: 0,
            total: (u64::from(sample_rate) * u64::from(TEST_TONE_MS) / 1000) as u32,
        }
    }

    /// Whether a tone is sounding.
    #[cfg(test)]
    pub(crate) fn active(&self) -> bool {
        self.output.is_some()
    }

    /// Takes a pending request, then adds the tone to its bus. Each buffer is
    /// interleaved stereo with `frames` frames.
    pub(crate) fn process(
        &mut self,
        master: &mut [f32],
        headphones: &mut [f32],
        booth: &mut [f32],
    ) {
        if let Some(output) = ToneOutput::from_code(self.request.swap(0, Ordering::Acquire)) {
            self.output = Some(output);
            self.phase = 0.0;
            self.elapsed = 0;
        }
        let Some(output) = self.output else {
            return;
        };
        let bus = match output {
            ToneOutput::Master => master,
            ToneOutput::Headphones => headphones,
            ToneOutput::Booth => booth,
        };
        for frame in bus.chunks_exact_mut(2) {
            if self.elapsed >= self.total {
                self.output = None;
                return;
            }
            let remaining = (self.total - self.elapsed) as f32;
            let edge = (self.elapsed as f32).min(remaining);
            let envelope = (edge / self.fade).min(1.0);
            let value = (self.phase * std::f32::consts::TAU).sin() * TEST_TONE_LEVEL * envelope;
            self.phase = (self.phase + self.step).fract();
            self.elapsed += 1;
            frame[0] += value;
            frame[1] += value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn tone_lands_on_the_requested_bus_only() {
        let slot = tone_request();
        let mut tone = ToneRt::new(Arc::clone(&slot), 48_000);
        request(&slot, ToneOutput::Headphones);
        let (mut m, mut h, mut b) = (vec![0.0; 9600], vec![0.0; 9600], vec![0.0; 9600]);
        tone.process(&mut m, &mut h, &mut b);
        assert_eq!(peak(&m), 0.0);
        assert_eq!(peak(&b), 0.0);
        assert!(peak(&h) > 0.15 && peak(&h) <= TEST_TONE_LEVEL);
        assert!(h.chunks(2).all(|f| f[0] == f[1]));
    }

    #[test]
    fn tone_fades_and_stops() {
        let slot = tone_request();
        let mut tone = ToneRt::new(Arc::clone(&slot), 48_000);
        request(&slot, ToneOutput::Master);
        let mut rendered = Vec::new();
        for _ in 0..100 {
            let mut m = vec![0.0; 1024];
            let (mut h, mut b) = (vec![0.0; 1024], vec![0.0; 1024]);
            tone.process(&mut m, &mut h, &mut b);
            rendered.extend_from_slice(&m);
            if !tone.active() {
                break;
            }
        }
        // 900 ms at 48 kHz.
        let sounding = rendered
            .iter()
            .rposition(|s| *s != 0.0)
            .map_or(0, |i| i / 2 + 1);
        assert!((43_100..=43_200).contains(&sounding), "{sounding}");
        assert!(rendered[0].abs() < 1e-6, "starts silent");
        assert!(rendered[(sounding - 1) * 2].abs() < 0.01, "ends silent");
        assert!(!tone.active());
        let mut m = vec![0.0; 1024];
        let (mut h, mut b) = (vec![0.0; 1024], vec![0.0; 1024]);
        tone.process(&mut m, &mut h, &mut b);
        assert_eq!(peak(&m), 0.0, "silent after the end");
    }
}
