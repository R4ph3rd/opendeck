//! Adapters from a plugin instance to the contracts traits:
//! [`PluginEffect`] implements [`Effect`], [`PluginInstrument`] implements
//! [`Instrument`]. Both wrap an [`AudioHalf`].
//!
//! - `prepare` (control thread) deactivates and re-activates the instance on
//!   the main thread and takes the new processor. If activation fails or
//!   times out, the adapter has no processor: an effect passes its input
//!   through (wet = dry), an instrument outputs silence. The instance handle
//!   reports [`InstanceStatus::Failed`].
//! - `set_parameter` (audio thread) queues into a preallocated list of
//!   [`MAX_PARAM_EVENTS`] at frame 0; editor edits arrive on a ring, keep
//!   their frame offset and join the list at the next block (sorted by offset).
//! - `latency_frames` returns the value read at activation. A plugin that
//!   changes latency later asks for a restart; app-core then re-installs the
//!   slot (the engine reads latency once at install, ADR 0006).
//! - Drop (control thread, after the engine returned the box) queues a
//!   main-thread job that deactivates and destroys the instance.

use std::sync::Arc;
use std::time::Duration;

use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::{Effect, EffectInfo, ParamId, ProcessContext, StereoBufferMut};
use opendeck_contracts::instrument::{Instrument, InstrumentInfo, NoteEvent};

use crate::InstanceStatus;
use crate::backend::{MAX_PARAM_EVENTS, ParamChange, PluginProcessor, ProcessBlock};
use crate::instances::{self, InstanceId};
use crate::main_thread::{MainThread, run_blocking};

/// The audio half shared by both adapters.
pub(crate) struct AudioHalf {
    pub id: InstanceId,
    pub main: Arc<dyn MainThread>,
    pub timeout: Duration,
    processor: Option<Box<dyn PluginProcessor>>,
    latency: u32,
    tail: Option<u32>,
    pending: Vec<ParamChange>,
    from_main: rtrb::Consumer<ParamChange>,
    to_main: rtrb::Producer<ParamChange>,
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
}

impl AudioHalf {
    pub fn new(
        id: InstanceId,
        main: Arc<dyn MainThread>,
        timeout: Duration,
        from_main: rtrb::Consumer<ParamChange>,
        to_main: rtrb::Producer<ParamChange>,
    ) -> Self {
        Self {
            id,
            main,
            timeout,
            processor: None,
            latency: 0,
            tail: None,
            pending: Vec::with_capacity(MAX_PARAM_EVENTS),
            from_main,
            to_main,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
        }
    }

    /// Re-activates on the main thread. Control thread; blocks up to the timeout.
    fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize, scratch: bool) {
        let old = self.processor.take();
        let id = self.id;
        let result = run_blocking(self.main.as_ref(), self.timeout, move || {
            instances::with(id, move |e| {
                if let Some(p) = old {
                    e.instance.deactivate(p);
                }
                match e.instance.activate(sample_rate, max_block_frames) {
                    Ok(p) => {
                        e.status = InstanceStatus::Active;
                        Ok((p, e.instance.latency_frames(), e.instance.tail_frames()))
                    }
                    Err(err) => {
                        e.status = InstanceStatus::Failed(err.to_string());
                        Err(err)
                    }
                }
            })
        });
        match result {
            Ok(Ok(Ok((p, latency, tail)))) => {
                self.processor = Some(p);
                self.latency = latency;
                self.tail = tail;
            }
            Ok(Ok(Err(_)) | Err(_)) => self.latency = 0,
            Err(err) => {
                self.latency = 0;
                // Timed out or no main thread: record it if the job still runs.
                let msg = err.to_string();
                let _ = self.main.spawn(Box::new(move || {
                    let _ = instances::with(id, |e| e.status = InstanceStatus::Failed(msg));
                }));
            }
        }
        let frames = if scratch { max_block_frames } else { 0 };
        self.scratch_l = vec![0.0; frames];
        self.scratch_r = vec![0.0; frames];
    }

    /// Queues a change, coalescing by id. RT-safe.
    fn set_parameter(&mut self, id: ParamId, normalized: f32) {
        if !normalized.is_finite() {
            return;
        }
        queue(&mut self.pending, id, normalized.clamp(0.0, 1.0), 0);
    }

    fn reset(&mut self) {
        self.pending.clear();
        if let Some(p) = self.processor.as_mut() {
            p.reset();
        }
    }

    /// Runs one block. `effect` copies the output buffer to the input
    /// scratch first. Returns false (output untouched) without a processor
    /// or for a block larger than prepared. RT-safe.
    fn process(
        &mut self,
        mut io: StereoBufferMut<'_>,
        notes: &[NoteEvent],
        ctx: &ProcessContext,
        effect: bool,
    ) -> bool {
        while let Ok(c) = self.from_main.pop() {
            if c.normalized.is_finite() {
                queue(
                    &mut self.pending,
                    c.id,
                    c.normalized.clamp(0.0, 1.0),
                    c.offset,
                );
            }
        }
        // `ProcessBlock::params`: offsets inside the block, sorted. The sort
        // is unstable so it does not allocate; changes with the same offset
        // are for different ids.
        let last = u32::try_from(io.frames().saturating_sub(1)).unwrap_or(u32::MAX);
        for c in &mut self.pending {
            c.offset = c.offset.min(last);
        }
        self.pending.sort_unstable_by_key(|c| c.offset);
        let Self {
            processor,
            pending,
            to_main,
            scratch_l,
            scratch_r,
            ..
        } = self;
        let Some(processor) = processor.as_mut() else {
            pending.clear();
            return false;
        };
        let frames = io.frames();
        let input = if effect {
            let (Some(sl), Some(sr)) = (scratch_l.get_mut(..frames), scratch_r.get_mut(..frames))
            else {
                pending.clear();
                return false;
            };
            let (l, r) = io.channels();
            sl.copy_from_slice(l);
            sr.copy_from_slice(r);
            Some((&*sl, &*sr))
        } else {
            None
        };
        processor.process(ProcessBlock {
            input,
            output: io,
            params: pending,
            notes,
            params_out: to_main,
            ctx,
        });
        pending.clear();
        true
    }
}

/// Coalescing push without reallocation: a change for the same id at the
/// same frame replaces the earlier one. RT-safe.
fn queue(pending: &mut Vec<ParamChange>, id: ParamId, normalized: f32, offset: u32) {
    if let Some(p) = pending
        .iter_mut()
        .find(|p| p.id == id && p.offset == offset)
    {
        p.normalized = normalized;
    } else if pending.len() < pending.capacity() {
        pending.push(ParamChange {
            id,
            normalized,
            offset,
        });
    }
}

impl Drop for AudioHalf {
    fn drop(&mut self) {
        let processor = self.processor.take();
        let id = self.id;
        // If the executor is gone the processor and instance drop where
        // they are; nothing else can be done.
        let _ = self.main.spawn(Box::new(move || {
            if let (Some(mut entry), Some(p)) = (instances::remove(id), processor) {
                entry.instance.deactivate(p);
            }
        }));
    }
}

/// A plugin effect. See the module docs.
pub struct PluginEffect {
    pub(crate) info: EffectInfo,
    pub(crate) half: AudioHalf,
}

impl Effect for PluginEffect {
    fn info(&self) -> &EffectInfo {
        &self.info
    }

    fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize) {
        self.half.prepare(sample_rate, max_block_frames, true);
        // A plugin that reports a zero tail needs no tail processing after
        // bypass; one that reports none is assumed to have one.
        self.info.has_tail = self.half.tail != Some(0);
    }

    fn reset(&mut self) {
        self.half.reset();
    }

    fn set_parameter(&mut self, id: ParamId, normalized: f32) {
        self.half.set_parameter(id, normalized);
    }

    fn process(&mut self, io: StereoBufferMut<'_>, ctx: &ProcessContext) {
        // Without a processor the input stays in place: pass-through.
        let _ = self.half.process(io, &[], ctx, true);
    }

    fn latency_frames(&self) -> u32 {
        self.half.latency
    }
}

/// A plugin instrument. See the module docs.
pub struct PluginInstrument {
    pub(crate) info: InstrumentInfo,
    pub(crate) half: AudioHalf,
}

impl Instrument for PluginInstrument {
    fn info(&self) -> &InstrumentInfo {
        &self.info
    }

    fn prepare(&mut self, sample_rate: SampleRate, max_block_frames: usize) {
        self.half.prepare(sample_rate, max_block_frames, false);
    }

    fn reset(&mut self) {
        self.half.reset();
    }

    fn set_parameter(&mut self, id: ParamId, normalized: f32) {
        self.half.set_parameter(id, normalized);
    }

    fn process(&mut self, mut out: StereoBufferMut<'_>, notes: &[NoteEvent], ctx: &ProcessContext) {
        let rendered = {
            let (l, r) = out.channels();
            StereoBufferMut::new(l, r).is_some_and(|io| self.half.process(io, notes, ctx, false))
        };
        if !rendered {
            let (l, r) = out.channels();
            l.fill(0.0);
            r.fill(0.0);
        }
    }

    fn latency_frames(&self) -> u32 {
        self.half.latency
    }
}
