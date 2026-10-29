//! Shared test helpers: scratch dirs, synthetic WAV files, a fake analyzer.
//! Nothing here ships audio; every fixture is synthesised at test time.
#![allow(dead_code, missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_contracts::analysis::{
    AnalysisError, AnalysisFeatures, AnalysisResult, Analyzer, AudioView, CancelFlag, Loudness,
};
use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::ids::JobId;
use opendeck_contracts::jobs::JobProgress;
use opendeck_contracts::key::MusicalKey;
use opendeck_contracts::peaks::{PeakLevel, PeakPoint};
use opendeck_contracts::units::{Bpm, Seconds};

/// Fresh scratch directory for one test.
pub fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-library-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tmp dir");
    dir
}

/// Mono 16-bit PCM WAV, a sine of `freq` Hz lasting `secs`.
pub fn write_wav(path: &Path, rate: u32, secs: f32, freq: f32) {
    let n = (rate as f32 * secs) as usize;
    let data_len = (n * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend(b"RIFF");
    b.extend((36 + data_len).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes()); // PCM
    b.extend(1u16.to_le_bytes()); // mono
    b.extend(rate.to_le_bytes());
    b.extend((rate * 2).to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data_len.to_le_bytes());
    for i in 0..n {
        let s = (std::f64::consts::TAU * f64::from(freq) * i as f64 / f64::from(rate)).sin();
        b.extend(((s * 0.5 * 32767.0) as i16).to_le_bytes());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, b).expect("write wav");
}

/// Analyzer that fills a fixed result. `delay` is spent in 2 ms slices that
/// poll the cancel flag, like a real analyzer polls between blocks.
pub struct FakeAnalyzer {
    pub delay: Duration,
    pub calls: AtomicUsize,
    pub version: u32,
}

impl FakeAnalyzer {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            calls: AtomicUsize::new(0),
            version: 1,
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Analyzer for FakeAnalyzer {
    fn id(&self) -> &str {
        "fake"
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn features(&self) -> AnalysisFeatures {
        AnalysisFeatures::ALL
    }
    fn analyze(
        &self,
        audio: AudioView<'_>,
        out: &mut AnalysisResult,
        cancel: &CancelFlag,
        _progress: &mut dyn FnMut(f32),
    ) -> Result<(), AnalysisError> {
        if !audio.is_valid() {
            return Err(AnalysisError::InvalidInput("bad view".into()));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        let start = Instant::now();
        while start.elapsed() < self.delay {
            if cancel.is_cancelled() {
                return Err(AnalysisError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        out.bpm = Some(Bpm(120.0));
        out.beat_grid = BeatGrid::new(Seconds(0.1), Bpm(120.0), 4).ok();
        out.key = MusicalKey::from_camelot("8A").ok();
        out.loudness = Some(Loudness {
            integrated_lufs: -14.0,
            true_peak_dbtp: -1.0,
        });
        out.peaks = vec![
            PeakLevel {
                sample_rate: audio.sample_rate.0,
                frames_per_point: 256,
                points: vec![
                    PeakPoint {
                        peak: 200,
                        low: 10,
                        mid: 20,
                        high: 30,
                    };
                    4
                ],
            },
            PeakLevel {
                sample_rate: audio.sample_rate.0,
                frames_per_point: 4096,
                points: vec![
                    PeakPoint {
                        peak: 100,
                        low: 5,
                        mid: 6,
                        high: 7,
                    };
                    2
                ],
            },
        ];
        Ok(())
    }
}

/// Reads progress events until `job` reaches a terminal state; returns all events seen.
pub fn wait_terminal(
    rx: &Receiver<JobProgress>,
    job: JobId,
    timeout: Duration,
) -> Vec<JobProgress> {
    let deadline = Instant::now() + timeout;
    let mut seen = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let ev = rx.recv_timeout(left).unwrap_or_else(|_| {
            panic!(
                "job {job:?} did not finish in {timeout:?}; saw {} events",
                seen.len()
            )
        });
        if ev.job == job {
            let done =
                ev.state.is_terminal() || ev.state == opendeck_contracts::jobs::JobState::Paused;
            seen.push(ev);
            if done {
                return seen;
            }
        }
    }
}
