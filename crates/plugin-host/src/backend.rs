//! The format seam: what a plugin format (CLAP, VST3, the fake test format)
//! implements so the generic adapters in this crate can host it.
//!
//! Three objects, split by thread like CLAP and VST3 split them:
//!
//! - [`PluginBackend`]: one per format. Lists default folders, scans files
//!   (in the scanner subprocess) and instantiates plugins (main thread).
//!   `Send + Sync`, shared by `Arc`.
//! - [`PluginInstance`]: the main-thread half of one plugin instance. Not
//!   `Send`: it lives in the main thread's instance table for its whole life
//!   (CLAP `[main-thread]` calls, VST3 UI-thread calls). Activation,
//!   parameters info, latency, state and (wave 9a-3) the editor go through it.
//! - [`PluginProcessor`]: the audio-thread half, created by
//!   [`PluginInstance::activate`] and given back to
//!   [`PluginInstance::deactivate`]. `Send`; moves to the audio thread inside
//!   an effect or instrument box. Its methods are RT-safe.
//!
//! Units: sample rates in Hz ([`SampleRate`]), sizes and offsets in frames,
//! parameter values normalized `[0, 1]` (backends convert CLAP plain values
//! with the descriptor).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::{ParamDescriptor, ParamId, ProcessContext, StereoBufferMut};
use opendeck_contracts::instrument::NoteEvent;
use opendeck_contracts::plugin::PluginInfo;

use crate::PluginError;

/// Most parameter changes one instance takes per block, from automation,
/// MIDI mapping and the editor together. Further changes to a parameter
/// already queued in the block replace its value; changes to new ids past
/// the limit are dropped.
pub const MAX_PARAM_EVENTS: usize = 256;

/// Capacity of each parameter ring between the main thread and one
/// instance's audio half (editor edits in, plugin output changes out).
pub const PARAM_RING: usize = 1024;

/// Called by a plugin instance (from any thread) when the plugin asks for the
/// main thread (CLAP `request_callback`, `request_restart`). The host queues a
/// main-thread job that calls [`PluginInstance::service`] on every live
/// instance. Cheap and idempotent: repeated calls before the job runs queue
/// one job. It allocates when it queues, so a plugin that calls it from its
/// audio callback breaks the no-allocation rule outside our control (ADR 0008).
pub type HostWake = Arc<dyn Fn() + Send + Sync>;

/// What an instance asked of the host since the last
/// [`PluginInstance::service`] call. All flags are sticky until read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InstanceRequests {
    /// The plugin asked to be deactivated and activated again (CLAP
    /// `request_restart`). app-core saves state, recreates and re-installs.
    pub restart: bool,
    /// The plugin reported a latency change (CLAP `latency.changed`).
    pub latency_changed: bool,
    /// The plugin's parameter list or values changed (CLAP `params.rescan`).
    pub params_changed: bool,
    /// The plugin state changed (CLAP `state.mark_dirty`).
    pub state_dirty: bool,
}

impl InstanceRequests {
    /// True if no request is pending.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Union of two sets of requests.
    pub fn merge(self, other: Self) -> Self {
        Self {
            restart: self.restart || other.restart,
            latency_changed: self.latency_changed || other.latency_changed,
            params_changed: self.params_changed || other.params_changed,
            state_dirty: self.state_dirty || other.state_dirty,
        }
    }
}

/// One parameter change. RT-safe (`Copy`).
///
/// `normalized` is in `[0, 1]`; `offset` is the frame inside the block
/// (the adapters queue changes at offset 0; sample-accurate automation can
/// use later offsets).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamChange {
    /// Parameter id (CLAP `clap_id`, VST3 `ParamID`).
    pub id: ParamId,
    /// Normalized value.
    pub normalized: f32,
    /// Frame inside the block.
    pub offset: u32,
}

/// Where a processor reports parameter changes made by the plugin itself
/// (CLAP output events, VST3 output parameter changes). RT-safe: a full sink
/// drops the change.
pub trait ParamSink {
    /// Queues one change; drops it if there is no room. RT-safe.
    fn push(&mut self, change: ParamChange);
}

impl ParamSink for rtrb::Producer<ParamChange> {
    fn push(&mut self, change: ParamChange) {
        // A full ring drops the change: the main thread is behind.
        let _ = rtrb::Producer::push(self, change);
    }
}

/// One block for a [`PluginProcessor`].
///
/// Invariants: `input` (if any) has `output.frames()` frames per channel;
/// `params` and `notes` are sorted by offset and every offset is
/// `< output.frames()`; `notes.len() <= MAX_NOTE_EVENTS` and
/// `params.len() <= MAX_PARAM_EVENTS`.
pub struct ProcessBlock<'a> {
    /// Stereo input (effects); `None` for instruments.
    pub input: Option<(&'a [f32], &'a [f32])>,
    /// Stereo output, to be overwritten.
    pub output: StereoBufferMut<'a>,
    /// Parameter changes for this block.
    pub params: &'a [ParamChange],
    /// Note events for this block (instruments).
    pub notes: &'a [NoteEvent],
    /// Changes the plugin makes itself.
    pub params_out: &'a mut dyn ParamSink,
    /// Tempo and transport.
    pub ctx: &'a ProcessContext,
}

/// One plugin format. Implementations are stateless or internally
/// synchronized; methods state the thread they run on.
pub trait PluginBackend: Send + Sync {
    /// Provider id, also the `provider` of every key this backend makes
    /// (`"clap"`, `"vst3"`, `"fake"`).
    fn provider(&self) -> &'static str;

    /// File or bundle extension without the dot (`"clap"`, `"vst3"`).
    fn extension(&self) -> &'static str;

    /// Default folders of this format on the running OS, before user
    /// folders. Any thread.
    fn default_folders(&self) -> Vec<PathBuf>;

    /// Lists the plugins in one file or bundle. Runs only in the scanner
    /// subprocess, on its main thread: loading a library runs its code, and
    /// a crash here must not take the app down.
    fn scan_file(&self, path: &Path) -> Result<Vec<PluginInfo>, PluginError>;

    /// Creates an instance of a scanned plugin. Main thread only. `wake` is
    /// the instance's way to ask for a [`PluginInstance::service`] call.
    fn instantiate(
        &self,
        info: &PluginInfo,
        wake: HostWake,
    ) -> Result<Box<dyn PluginInstance>, PluginError>;
}

/// Main-thread half of one plugin instance. Every method runs on the main
/// thread and may allocate and block. Dropping it destroys the plugin; the
/// host deactivates it first.
pub trait PluginInstance {
    /// Parameters, in display order, each satisfying
    /// [`ParamDescriptor::is_valid`]. Plugin parameters that cannot be
    /// described (empty range) are left out.
    fn params(&self) -> Vec<ParamDescriptor>;

    /// Activates for `sample_rate` and blocks of `1..=max_block_frames`
    /// frames and returns the audio half. Fails if already active.
    fn activate(
        &mut self,
        sample_rate: SampleRate,
        max_block_frames: usize,
    ) -> Result<Box<dyn PluginProcessor>, PluginError>;

    /// Deactivates, taking the audio half back. The processor must have
    /// left the audio thread.
    fn deactivate(&mut self, processor: Box<dyn PluginProcessor>);

    /// Latency in frames at the active rate; 0 while inactive.
    fn latency_frames(&self) -> u32;

    /// Tail length in frames as of the last activation: `Some(0)` for a
    /// plugin that reports no tail, `None` if it does not say (treated as
    /// "may have a tail"). `u32::MAX` means infinite.
    fn tail_frames(&self) -> Option<u32> {
        None
    }

    /// Runs the plugin's main-thread callback if it asked for one and
    /// returns what it asked of the host since the last call.
    fn service(&mut self) -> InstanceRequests {
        InstanceRequests::default()
    }

    /// Serializes the full plugin state (CLAP `state`, VST3 component and
    /// controller state). Opaque bytes, stored with presets.
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError>;

    /// Restores bytes from [`PluginInstance::save_state`] of the same plugin.
    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError>;
}

/// Audio-thread half of an active plugin instance.
///
/// Real-time rules: [`PluginProcessor::process`] and
/// [`PluginProcessor::reset`] must not allocate, lock, block, do I/O, log or
/// panic in the backend code; what the plugin binary does inside is outside
/// our control (ADR 0008, in-process risk). Dropping a processor outside
/// [`PluginInstance::deactivate`] (a timed-out activation) happens on the
/// main thread and must be safe there.
pub trait PluginProcessor: Send {
    /// Processes one block. Overwrites `block.output`.
    fn process(&mut self, block: ProcessBlock<'_>);

    /// Clears internal state and stops voices (CLAP `reset`, VST3 flush by
    /// processing with no input after `setProcessing(false/true)`).
    fn reset(&mut self);
}
