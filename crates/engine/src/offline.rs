//! Device-free renderer that drives the same graph as the audio callback.
//!
//! Used by tests and tools: send commands through [`handle`](OfflineRenderer::handle),
//! render block by block, inspect the samples and telemetry. Every block goes
//! through [`EngineCore::render`], so tests run under `assert_no_alloc` too.

use std::time::{Duration, Instant};

use crate::audio::EngineCore;
use crate::handle::{EngineHandle, engine};
use crate::{ConfigError, EngineConfig};

/// Timing of the blocks rendered so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RenderStats {
    /// Blocks rendered.
    pub blocks: u64,
    /// Frames rendered.
    pub frames: u64,
    /// Total CPU (wall) time spent in `render`.
    pub total: Duration,
    /// Slowest single block.
    pub worst_block: Duration,
}

impl RenderStats {
    /// Mean time per block.
    pub fn mean_block(&self) -> Duration {
        match u32::try_from(self.blocks) {
            Ok(0) => Duration::ZERO,
            Ok(n) => self.total / n,
            Err(_) => self.total / u32::MAX,
        }
    }

    /// Render time divided by the audio duration rendered at `sample_rate`
    /// (`1.0` = just keeping up).
    pub fn realtime_fraction(&self, sample_rate: u32) -> f64 {
        if self.frames == 0 || sample_rate == 0 {
            return 0.0;
        }
        self.total.as_secs_f64() / (self.frames as f64 / f64::from(sample_rate))
    }
}

/// An engine without a device.
pub struct OfflineRenderer {
    core: EngineCore,
    handle: EngineHandle,
    block_frames: usize,
    stats: RenderStats,
}

impl OfflineRenderer {
    /// Builds an engine for `config`. Blocks default to `max_block_frames`.
    pub fn new(config: EngineConfig) -> Result<Self, ConfigError> {
        let (core, handle) = engine(config)?;
        Ok(Self {
            core,
            handle,
            block_frames: config.max_block_frames as usize,
            stats: RenderStats::default(),
        })
    }

    /// Sets the block size used by [`render`](Self::render) and
    /// [`render_into`](Self::render_into). Blocks larger than the engine's
    /// `max_block_frames` are split by the core.
    pub fn with_block_frames(mut self, frames: usize) -> Self {
        self.block_frames = frames.max(1);
        self
    }

    /// Control-side endpoints: send commands, load tracks, read telemetry.
    pub fn handle(&mut self) -> &mut EngineHandle {
        &mut self.handle
    }

    /// The audio-side core, for inspection.
    pub fn core(&self) -> &EngineCore {
        &self.core
    }

    /// Timing of everything rendered so far.
    pub fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Renders `out` (interleaved stereo master) in blocks of the configured
    /// size, applying queued commands at each block boundary. Allocation free.
    pub fn render_into(&mut self, out: &mut [f32]) {
        for block in out.chunks_mut(self.block_frames * 2) {
            let started = Instant::now();
            self.core.render(block);
            let elapsed = started.elapsed();
            self.stats.blocks += 1;
            self.stats.frames += (block.len() / 2) as u64;
            self.stats.total += elapsed;
            self.stats.worst_block = self.stats.worst_block.max(elapsed);
        }
    }

    /// Renders `frames` frames and returns them as interleaved stereo.
    pub fn render(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        self.render_into(&mut out);
        out
    }
}
