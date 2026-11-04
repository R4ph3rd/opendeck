//! The audio half of a VST3 instance. Everything here runs on the audio
//! thread (except construction and drop) and allocates nothing: buffers,
//! bus tables, parameter queues and event lists are created at activation.

use std::ptr;
use std::sync::Arc;

use opendeck_contracts::fx::{ParamId, ProcessContext};
use opendeck_contracts::instrument::MAX_NOTE_EVENTS;
use opendeck_contracts::plugin::PluginKind;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, BusInfo, BusTypes_, IAudioProcessor,
    IAudioProcessorTrait, IComponent, IComponentTrait, MediaTypes_,
    ProcessContext as VstProcessContext, ProcessContext_::StatesAndFlags_, ProcessData,
    ProcessModes_, SpeakerArrangement, SymbolicSampleSizes_,
};
use vst3::{ComPtr, ComWrapper};

use super::events::EventList;
use super::param_changes::ParamChanges;
use super::util::{e, is_ok};
use crate::PluginError;
use crate::backend::{MAX_PARAM_EVENTS, ParamChange, PluginProcessor, ProcessBlock};

/// Distinct parameters a plugin may report per block through its output
/// parameter changes; more are dropped.
pub(super) const MAX_OUT_PARAMS: usize = 128;

/// One audio bus as the component declares it.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Bus {
    /// The bus we feed (the main bus; effects: input and output).
    pub is_main: bool,
}

/// The buses of a component.
#[derive(Debug, Clone, Default)]
pub(super) struct Layout {
    pub audio_in: Vec<Bus>,
    pub audio_out: Vec<Bus>,
    /// Index of the main audio input; `None` for instruments.
    pub main_in: Option<usize>,
    /// Index of the main audio output.
    pub main_out: usize,
    /// Number of event input buses (note input).
    pub event_inputs: i32,
}

impl Layout {
    /// Reads the bus lists. Effects need audio in and out, instruments an
    /// audio output and an event input.
    pub(super) fn read(
        component: &ComPtr<IComponent>,
        kind: PluginKind,
    ) -> Result<Self, PluginError> {
        let read = |dir: i32| -> Vec<(bool, i32)> {
            // SAFETY: plain calls on a live component; `info` is plain data.
            let count = unsafe { component.getBusCount(e(MediaTypes_::kAudio), dir) }.max(0);
            (0..count)
                .filter_map(|i| {
                    let mut info: BusInfo = unsafe { std::mem::zeroed() };
                    // SAFETY: valid out pointer.
                    is_ok(unsafe {
                        component.getBusInfo(e(MediaTypes_::kAudio), dir, i, &mut info)
                    })
                    .then_some((info.busType == e(BusTypes_::kMain), info.channelCount))
                })
                .collect()
        };
        let ins = read(e(BusDirections_::kInput));
        let outs = read(e(BusDirections_::kOutput));
        let main_of = |buses: &[(bool, i32)]| buses.iter().position(|b| b.0).unwrap_or(0);
        let effect = kind == PluginKind::Effect;
        // SAFETY: plain call on a live component.
        let event_inputs =
            unsafe { component.getBusCount(e(MediaTypes_::kEvent), e(BusDirections_::kInput)) }
                .max(0);
        if outs.is_empty() || (effect && ins.is_empty()) {
            return Err(PluginError::Plugin(
                "no audio input and output bus for an effect".into(),
            ));
        }
        if !effect && event_inputs == 0 {
            return Err(PluginError::Plugin(
                "instrument has no event input bus".into(),
            ));
        }
        let (main_in, main_out) = (main_of(&ins), main_of(&outs));
        let mark = |buses: &[(bool, i32)], main: Option<usize>| -> Vec<Bus> {
            (0..buses.len())
                .map(|i| Bus {
                    is_main: Some(i) == main,
                })
                .collect()
        };
        Ok(Self {
            audio_in: mark(&ins, effect.then_some(main_in)),
            audio_out: mark(&outs, Some(main_out)),
            main_in: effect.then_some(main_in),
            main_out,
            event_inputs,
        })
    }

    /// The arrangements the plugin reports now, one per bus (0 if it does
    /// not say), as the base for `setBusArrangements`.
    pub(super) fn current_arrangements(
        &self,
        audio: &ComPtr<IAudioProcessor>,
        input: bool,
    ) -> Vec<SpeakerArrangement> {
        let (dir, buses) = if input {
            (e(BusDirections_::kInput), &self.audio_in)
        } else {
            (e(BusDirections_::kOutput), &self.audio_out)
        };
        (0..buses.len())
            .map(|i| {
                let mut arr: SpeakerArrangement = 0;
                // SAFETY: valid out pointer.
                if is_ok(unsafe { audio.getBusArrangement(dir, i as i32, &mut arr) }) {
                    arr
                } else {
                    0
                }
            })
            .collect()
    }
}

/// Audio half of a VST3 plugin instance. See the module docs of `vst3`.
pub struct Vst3Processor {
    audio: ComPtr<IAudioProcessor>,
    effect: bool,
    max_frames: usize,
    /// Private copy of the input (plugins may write to input buffers).
    in_data: [Vec<f32>; 2],
    /// Channel pointer tables. Boxed so their addresses survive moves of
    /// `self`; `AudioBusBuffers` point into them.
    in_chan: Box<[*mut f32; 2]>,
    out_chan: Box<[*mut f32; 2]>,
    in_buses: Vec<AudioBusBuffers>,
    out_buses: Vec<AudioBusBuffers>,
    in_main: Option<usize>,
    out_main: usize,
    in_changes: ComWrapper<ParamChanges>,
    out_changes: ComWrapper<ParamChanges>,
    events_in: ComWrapper<EventList>,
    events_out: ComWrapper<EventList>,
    context: Box<VstProcessContext>,
    /// Ids the host may set (sorted).
    settable: Arc<[u32]>,
    /// Edits made in the plugin's editor.
    edits: rtrb::Consumer<ParamChange>,
    /// Mirror of every value applied or reported, for the controller.
    sync: rtrb::Producer<ParamChange>,
    position: i64,
    // Last: keeps the library loaded as long as this exists.
    _module: Arc<super::module::Module>,
}

// SAFETY: the processor moves between threads only whole. The COM objects
// are used by one thread at a time (the audio thread, or the main thread
// after the audio thread released it); the raw pointers in the bus tables
// point into this struct's own boxes or are set from the caller's buffers
// inside `process` and not used after it returns. The module reference
// counts atomically.
unsafe impl Send for Vst3Processor {}

fn blank_bus() -> AudioBusBuffers {
    AudioBusBuffers {
        numChannels: 0,
        silenceFlags: 0,
        __field0: AudioBusBuffers__type0 {
            channelBuffers32: ptr::null_mut(),
        },
    }
}

impl Vst3Processor {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        audio: ComPtr<IAudioProcessor>,
        module: Arc<super::module::Module>,
        layout: &Layout,
        effect: bool,
        max_frames: usize,
        in_changes: ComWrapper<ParamChanges>,
        out_changes: ComWrapper<ParamChanges>,
        events_in: ComWrapper<EventList>,
        events_out: ComWrapper<EventList>,
        settable: Arc<[u32]>,
        edits: rtrb::Consumer<ParamChange>,
        sync: rtrb::Producer<ParamChange>,
    ) -> Self {
        // SAFETY: `ProcessContext` is plain data; all-zero is valid.
        let context: Box<VstProcessContext> = Box::new(unsafe { std::mem::zeroed() });
        Self {
            audio,
            effect,
            max_frames,
            in_data: [vec![0.0; max_frames], vec![0.0; max_frames]],
            in_chan: Box::new([ptr::null_mut(); 2]),
            out_chan: Box::new([ptr::null_mut(); 2]),
            in_buses: vec![blank_bus(); layout.audio_in.len()],
            out_buses: vec![blank_bus(); layout.audio_out.len()],
            in_main: layout.main_in,
            out_main: layout.main_out,
            in_changes,
            out_changes,
            events_in,
            events_out,
            context,
            settable,
            edits,
            sync,
            position: 0,
            _module: module,
        }
    }

    /// Writes the input to the output (effects) or silence.
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

    fn fill_params(&mut self, block: &ProcessBlock<'_>, frames: usize) {
        self.in_changes.clear();
        self.out_changes.clear();
        let last = u32::try_from(frames.saturating_sub(1)).unwrap_or(u32::MAX);
        for c in block.params.iter().take(MAX_PARAM_EVENTS) {
            if !c.normalized.is_finite() || self.settable.binary_search(&c.id.0).is_err() {
                continue;
            }
            let v = c.normalized.clamp(0.0, 1.0);
            if self
                .in_changes
                .push(c.id.0, c.offset.min(last), f64::from(v))
            {
                let _ = self.sync.push(ParamChange {
                    id: c.id,
                    normalized: v,
                    offset: c.offset.min(last),
                });
            }
        }
        // Editor edits come from the plugin itself: no id filter.
        while let Ok(c) = self.edits.pop() {
            let _ = self.in_changes.push(c.id.0, 0, f64::from(c.normalized));
        }
    }

    fn fill_context(&mut self, ctx: &ProcessContext) {
        let mut state = 0;
        let c = &mut *self.context;
        c.sampleRate = f64::from(ctx.sample_rate.0);
        c.projectTimeSamples = self.position;
        c.continousTimeSamples = self.position;
        c.tempo = 0.0;
        c.projectTimeMusic = 0.0;
        if ctx.playing {
            state |= e(StatesAndFlags_::kPlaying) as u32;
        }
        if let Some(bpm) = ctx.bpm.filter(|b| b.is_valid()) {
            state |= e(StatesAndFlags_::kTempoValid) as u32;
            c.tempo = bpm.0;
        }
        if let Some(pos) = ctx.beat_position.filter(|p| p.0.is_finite()) {
            state |= e(StatesAndFlags_::kProjectTimeMusicValid) as u32;
            c.projectTimeMusic = pos.0;
        }
        c.state = state;
    }
}

impl PluginProcessor for Vst3Processor {
    fn process(&mut self, mut block: ProcessBlock<'_>) {
        let frames = block.output.frames();
        if frames == 0 {
            return;
        }
        if frames > self.max_frames {
            self.fallback(&mut block);
            return;
        }

        // Input: a private copy in the main input bus (zeros if absent).
        if self.effect {
            let [dl, dr] = &mut self.in_data;
            match block.input {
                Some((il, ir)) => {
                    dl[..frames].copy_from_slice(il);
                    dr[..frames].copy_from_slice(ir);
                }
                None => {
                    dl[..frames].fill(0.0);
                    dr[..frames].fill(0.0);
                }
            }
            self.in_chan[0] = dl.as_mut_ptr();
            self.in_chan[1] = dr.as_mut_ptr();
            if let Some(bus) = self.in_main.and_then(|i| self.in_buses.get_mut(i)) {
                bus.numChannels = 2;
                bus.silenceFlags = 0;
                bus.__field0.channelBuffers32 = self.in_chan.as_mut_ptr();
            }
        }
        // Output: the main bus writes straight into the caller's buffers.
        {
            let (l, r) = block.output.channels();
            self.out_chan[0] = l.as_mut_ptr();
            self.out_chan[1] = r.as_mut_ptr();
        }
        if let Some(bus) = self.out_buses.get_mut(self.out_main) {
            bus.numChannels = 2;
            bus.silenceFlags = 0;
            bus.__field0.channelBuffers32 = self.out_chan.as_mut_ptr();
        }

        self.fill_params(&block, frames);
        self.events_in.clear();
        self.events_out.clear();
        for n in block.notes.iter().take(MAX_NOTE_EVENTS) {
            let _ = self.events_in.push_note(n);
        }
        self.fill_context(block.ctx);

        let mut data = ProcessData {
            processMode: e(ProcessModes_::kRealtime),
            symbolicSampleSize: e(SymbolicSampleSizes_::kSample32),
            numSamples: i32::try_from(frames).unwrap_or(i32::MAX),
            numInputs: i32::try_from(self.in_buses.len()).unwrap_or(0),
            numOutputs: i32::try_from(self.out_buses.len()).unwrap_or(0),
            inputs: if self.in_buses.is_empty() {
                ptr::null_mut()
            } else {
                self.in_buses.as_mut_ptr()
            },
            outputs: self.out_buses.as_mut_ptr(),
            inputParameterChanges: ParamChanges::as_ptr(&self.in_changes),
            outputParameterChanges: ParamChanges::as_ptr(&self.out_changes),
            inputEvents: EventList::as_ptr(&self.events_in),
            outputEvents: EventList::as_ptr(&self.events_out),
            processContext: &raw mut *self.context,
        };
        // SAFETY: `data` and everything it points to is valid for the call:
        // the bus tables and channel pointer tables are owned by `self`; the
        // main input points at our copy and the main output at the caller's
        // buffers, each `frames <= max_frames` samples; aux buses have no
        // channels; the lists are live COM objects owned by `self`.
        let result = unsafe { self.audio.process(&mut data) };
        self.position = self.position.wrapping_add(frames as i64);
        if !is_ok(result) {
            self.fallback(&mut block);
            return;
        }

        // A plugin may flag output channels silent without writing them.
        let silent = self
            .out_buses
            .get(self.out_main)
            .map_or(0, |b| b.silenceFlags);
        if silent & 0b11 != 0 {
            let (l, r) = block.output.channels();
            if silent & 0b01 != 0 {
                l.fill(0.0);
            }
            if silent & 0b10 != 0 {
                r.fill(0.0);
            }
        }

        let last = i32::try_from(frames.saturating_sub(1)).unwrap_or(i32::MAX);
        let out = &mut *block.params_out;
        let sync = &mut self.sync;
        self.out_changes.for_each(|id, offset, value| {
            let change = ParamChange {
                id: ParamId(id),
                normalized: value.clamp(0.0, 1.0) as f32,
                offset: offset.clamp(0, last) as u32,
            };
            out.push(change);
            let _ = sync.push(change);
        });
    }

    fn reset(&mut self) {
        // VST3 has no reset call. The documented way to clear a plugin's
        // internal state (delay lines, voices) is to stop and start
        // processing (`setProcessing(false)`, then `(true)`).
        // `TODO(verify)`: the SDK allows these from the audio thread.
        // SAFETY: plain calls on a live audio processor.
        unsafe {
            let _ = self.audio.setProcessing(0);
            let _ = self.audio.setProcessing(1);
        }
        self.position = 0;
    }
}
