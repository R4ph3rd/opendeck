//! The audio half of a CLAP instance. Everything here runs on the audio
//! thread (except construction and drop) and allocates nothing.

use std::ptr;
use std::sync::Arc;

use clack_host::events::event_types::{
    NoteOffEvent, NoteOnEvent, ParamValueEvent, TransportEvent, TransportFlags,
};
use clack_host::events::io::{OutputEventBuffer, TryPushError};
use clack_host::events::spaces::CoreEventSpace;
use clack_host::events::{EventFlags, EventHeader, Match};
use clack_host::prelude::*;
use clack_host::utils::{BeatTime, SecondsTime};
use clap_sys::audio_buffer::clap_audio_buffer;
use opendeck_contracts::fx::{ParamId, ProcessContext};
use opendeck_contracts::instrument::{MAX_NOTE_EVENTS, NoteKind};

use super::host::Host;
use super::params::{ParamEntry, find};
use crate::backend::{MAX_PARAM_EVENTS, ParamChange, PluginProcessor, ProcessBlock};

/// Output parameter events kept per block; more are dropped.
const MAX_OUT_EVENTS: usize = 64;

/// One audio port as the plugin declares it.
#[derive(Debug, Clone, Copy)]
pub(super) struct PortSpec {
    pub channels: u32,
    pub is_main: bool,
}

/// Fixed-capacity collector for the parameter values a plugin reports.
struct ParamOut {
    items: [(u32, f64, u32); MAX_OUT_EVENTS],
    len: usize,
}

impl OutputEventBuffer for ParamOut {
    fn try_push(&mut self, event: &UnknownEvent) -> Result<(), TryPushError> {
        if let Some(CoreEventSpace::ParamValue(e)) = event.as_core_event()
            && let Some(id) = e.param_id()
        {
            let slot = self.items.get_mut(self.len).ok_or_else(TryPushError::new)?;
            *slot = (id.get(), e.value(), e.time());
            self.len += 1;
        }
        Ok(())
    }
}

/// The channel pointer table and `clap_audio_buffer` list of one direction.
///
/// `ptrs` and the `Vec`s in `data` never reallocate after `new`, so the
/// pointers in `cfg` and `ptrs` stay valid. The main output port has no
/// storage of its own: its two pointers are set to the caller's buffers for
/// each block.
struct Bank {
    cfg: Vec<clap_audio_buffer>,
    ptrs: Vec<*mut f32>,
    /// Backing storage per channel; empty for the main output channels.
    data: Vec<Vec<f32>>,
    /// Index in `ptrs` and `data` of the main port's first channel.
    main_first: Option<usize>,
}

impl Bank {
    fn new(ports: &[PortSpec], max_frames: usize, main_is_caller_owned: bool) -> Self {
        let total: usize = ports.iter().map(|p| p.channels as usize).sum();
        let mut ptrs: Vec<*mut f32> = vec![ptr::null_mut(); total];
        let mut data: Vec<Vec<f32>> = Vec::with_capacity(total);
        let mut main_first = None;
        for p in ports {
            if p.is_main {
                main_first = Some(data.len());
            }
            for _ in 0..p.channels {
                let caller = p.is_main && main_is_caller_owned;
                data.push(if caller {
                    Vec::new()
                } else {
                    vec![0.0; max_frames]
                });
            }
        }
        for (slot, buf) in ptrs.iter_mut().zip(data.iter_mut()) {
            if !buf.is_empty() {
                *slot = buf.as_mut_ptr();
            }
        }
        let base = ptrs.as_mut_ptr();
        let mut cfg = Vec::with_capacity(ports.len());
        let mut offset = 0usize;
        for p in ports {
            // SAFETY: `offset + channels <= total`, so the pointer stays inside `ptrs`.
            let first = unsafe { base.add(offset) };
            // Aux ports carry silence and say so; the main port is overwritten.
            let silent = !p.is_main && p.channels <= 64;
            cfg.push(clap_audio_buffer {
                data32: first,
                data64: ptr::null_mut(),
                channel_count: p.channels,
                latency: 0,
                constant_mask: if silent { (1u64 << p.channels) - 1 } else { 0 },
            });
            offset += p.channels as usize;
        }
        Self {
            cfg,
            ptrs,
            data,
            main_first,
        }
    }
}

/// Audio half of a CLAP plugin instance. See the module docs of `clap`.
pub struct ClapProcessor {
    /// `None` only while dropping.
    proc: Option<StartedPluginAudioProcessor<Host>>,
    params: Arc<[ParamEntry]>,
    inputs: Bank,
    outputs: Bank,
    max_frames: usize,
    /// Parameter and note events for the block.
    events: EventBuffer,
    out: ParamOut,
    steady: u64,
    /// The plugin takes audio input (effect) or not (instrument).
    effect: bool,
}

// SAFETY: the raw pointers in `Bank` point into heap buffers owned by the same
// struct, or are set from the caller's buffers inside `process` and not used
// after it returns. `StartedPluginAudioProcessor` is `Send`; nothing else is
// shared. The struct moves between threads only whole.
unsafe impl Send for ClapProcessor {}

impl ClapProcessor {
    pub(super) fn new(
        proc: StartedPluginAudioProcessor<Host>,
        params: Arc<[ParamEntry]>,
        in_ports: &[PortSpec],
        out_ports: &[PortSpec],
        max_frames: usize,
        effect: bool,
    ) -> Self {
        Self {
            proc: Some(proc),
            params,
            inputs: Bank::new(in_ports, max_frames, false),
            outputs: Bank::new(out_ports, max_frames, true),
            max_frames,
            events: EventBuffer::with_capacity(MAX_PARAM_EVENTS + MAX_NOTE_EVENTS + 1),
            out: ParamOut {
                items: [(0, 0.0, 0); MAX_OUT_EVENTS],
                len: 0,
            },
            steady: 0,
            effect,
        }
    }

    /// Mutable access to the started processor for extension queries at
    /// activation.
    pub(super) fn plugin_handle(&mut self) -> Option<PluginAudioProcessorHandle<'_>> {
        self.proc.as_mut().map(|p| p.plugin_handle())
    }

    /// Writes the plugin's input to the output (effects) or silence.
    fn fallback(&self, block: &mut ProcessBlock<'_>) {
        let (l, r) = block.output.channels();
        match block.input {
            Some((il, ir)) if self.effect => {
                l.copy_from_slice(il);
                r.copy_from_slice(ir);
            }
            _ => {
                l.fill(0.0);
                r.fill(0.0);
            }
        }
    }

    fn fill_events(&mut self, block: &ProcessBlock<'_>, frames: u32) {
        self.events.clear();
        let last = frames.saturating_sub(1);
        for c in block.params.iter().take(MAX_PARAM_EVENTS) {
            let Some(entry) = find(&self.params, c.id).filter(|e| e.settable) else {
                continue;
            };
            let Some(id) = ClapId::from_raw(c.id.0) else {
                continue;
            };
            let plain = f64::from(entry.desc.to_plain(c.normalized));
            self.events.push(&ParamValueEvent::new(
                c.offset.min(last),
                id,
                Pckn::match_all(),
                plain,
            ));
        }
        for n in block.notes.iter().take(MAX_NOTE_EVENTS) {
            let pckn = Pckn::new(0u16, u16::from(n.channel()), u16::from(n.key()), Match::All);
            let time = n.offset().min(last);
            match n.kind() {
                NoteKind::On => {
                    self.events
                        .push(&NoteOnEvent::new(time, pckn, f64::from(n.velocity())))
                }
                NoteKind::Off => {
                    self.events
                        .push(&NoteOffEvent::new(time, pckn, f64::from(n.velocity())))
                }
            }
        }
        self.events.sort();
    }
}

/// Transport as of the first frame of the block.
fn transport(ctx: &ProcessContext) -> TransportEvent {
    let mut flags = TransportFlags::empty();
    let mut tempo = 0.0;
    let mut beats = BeatTime::from_int(0);
    let mut seconds = SecondsTime::from_int(0);
    if let Some(bpm) = ctx.bpm.filter(|b| b.is_valid()) {
        flags |= TransportFlags::HAS_TEMPO;
        tempo = bpm.0;
        if let Some(pos) = ctx.beat_position.filter(|p| p.0.is_finite()) {
            flags |= TransportFlags::HAS_BEATS_TIMELINE | TransportFlags::HAS_SECONDS_TIMELINE;
            beats = BeatTime::from_float(pos.0);
            seconds = SecondsTime::from_float(pos.0 * 60.0 / bpm.0);
        }
    } else if let Some(pos) = ctx.beat_position.filter(|p| p.0.is_finite()) {
        flags |= TransportFlags::HAS_BEATS_TIMELINE;
        beats = BeatTime::from_float(pos.0);
    }
    if ctx.playing {
        flags |= TransportFlags::IS_PLAYING;
    }
    TransportEvent {
        header: EventHeader::new_core(0, EventFlags::empty()),
        flags,
        song_pos_beats: beats,
        song_pos_seconds: seconds,
        tempo,
        tempo_inc: 0.0,
        loop_start_beats: BeatTime::from_int(0),
        loop_end_beats: BeatTime::from_int(0),
        loop_start_seconds: SecondsTime::from_int(0),
        loop_end_seconds: SecondsTime::from_int(0),
        bar_start: BeatTime::from_int(0),
        bar_number: 0,
        time_signature_numerator: 0,
        time_signature_denominator: 0,
    }
}

impl PluginProcessor for ClapProcessor {
    fn process(&mut self, mut block: ProcessBlock<'_>) {
        let frames = block.output.frames();
        if frames == 0 {
            return;
        }
        if frames > self.max_frames || self.proc.is_none() {
            self.fallback(&mut block);
            return;
        }
        let frames_u32 = u32::try_from(frames).unwrap_or(u32::MAX);

        // Input: copy into the main input channels (the plugin may write to
        // its input buffers, and `block.input` is read-only).
        if let (Some((il, ir)), Some(first)) = (block.input, self.inputs.main_first) {
            if let Some(buf) = self.inputs.data.get_mut(first) {
                buf[..frames].copy_from_slice(il);
            }
            if let Some(buf) = self.inputs.data.get_mut(first + 1) {
                buf[..frames].copy_from_slice(ir);
            }
        }
        // Output: the main port writes straight into the caller's buffers.
        if let Some(first) = self.outputs.main_first {
            let (l, r) = block.output.channels();
            if let Some(slot) = self.outputs.ptrs.get_mut(first) {
                *slot = l.as_mut_ptr();
            }
            if let Some(slot) = self.outputs.ptrs.get_mut(first + 1) {
                *slot = r.as_mut_ptr();
            }
        }

        self.fill_events(&block, frames_u32);
        self.out.len = 0;
        let transport = transport(block.ctx);

        let Some(proc) = self.proc.as_mut() else {
            return;
        };
        // SAFETY: every `clap_audio_buffer` in the banks points at a channel
        // table of valid `*mut f32`, each valid for `frames <= max_frames`
        // samples: aux and input channels are our own buffers of
        // `max_frames`; the main output pointers were just set from the
        // caller's buffers of `frames` samples, which outlive this call.
        let (inputs, mut outputs) = unsafe {
            (
                InputAudioBuffers::from_raw_buffers(&self.inputs.cfg, frames_u32),
                OutputAudioBuffers::from_raw_buffers(&mut self.outputs.cfg, frames_u32),
            )
        };
        let result = proc.process(
            &inputs,
            &mut outputs,
            &self.events.as_input(),
            &mut OutputEvents::from_buffer(&mut self.out),
            Some(self.steady),
            Some(&transport),
        );
        self.steady = self.steady.wrapping_add(u64::from(frames_u32));
        if result.is_err() {
            self.fallback(&mut block);
        }

        for &(id, value, time) in self.out.items.iter().take(self.out.len) {
            if let Some(entry) = find(&self.params, ParamId(id)) {
                block.params_out.push(ParamChange {
                    id: ParamId(id),
                    normalized: entry.desc.to_normalized(value as f32),
                    offset: time.min(frames_u32.saturating_sub(1)),
                });
            }
        }
    }

    fn reset(&mut self) {
        if let Some(p) = self.proc.as_mut() {
            p.reset();
        }
        self.steady = 0;
    }
}

impl Drop for ClapProcessor {
    fn drop(&mut self) {
        // Leave the started state before the instance is deactivated. This
        // runs on the main thread after the audio thread released us.
        if let Some(p) = self.proc.take() {
            drop(p.stop_processing());
        }
    }
}
