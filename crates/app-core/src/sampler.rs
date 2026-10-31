//! The sampler: 16 slots (`SAMPLER_SLOTS`), their assignment, per-slot
//! settings and the hand-off to the engine (milestone 7).
//!
//! # Slot layout
//!
//! The FLX4 manual (p.62) maps deck 1's pads to slots 1-8 and deck 2's pads to
//! slots 9-16. That is the default, [`SamplerLayout::Split`]. Open question
//! Q05 asks whether the hardware banks are shared; [`SamplerLayout::Shared`]
//! makes both decks trigger slots 1-8. The choice is stored with the slots.
//!
//! # Engine hand-off
//!
//! Samples are decoded here (media crate, engine rate) and handed to the
//! engine through [`SamplerEngine`]. [`AppCore`](crate::AppCore) installs
//! [`EngineSampler`], which passes them to the control thread: it hands them
//! to the engine (`EngineHandle::load_sampler`, with the analysed BPM when
//! the sample is a library track) and replays them after an engine restart
//! at the same rate. After a rate change call
//! [`SamplerService::follow_rate`] to decode the slots again. The
//! [`StubSamplerEngine`] only remembers what it was given (tests). Triggers,
//! stops and the bus level use
//! `EngineCommand::{SamplerTrigger, SamplerStop, SetSamplerLevel}`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use opendeck_contracts::analysis::CancelFlag;
use opendeck_contracts::ids::{SAMPLER_SLOTS, SamplerSlotId, TrackId};
use opendeck_contracts::sampler::SamplerMode;
use opendeck_contracts::{Bpm, DeckId, EngineCommand, Gain, SampleRate};
use opendeck_engine::TrackBuffer;
use opendeck_library::Library;
use opendeck_media::decode_file;
use serde::{Deserialize, Serialize};

use crate::control::Msg;
use crate::pads::{Notifier, PadEvent};
use crate::types::CoreError;
use crate::{AudioController, LibraryService};

/// Library settings key of the stored sampler.
pub const SAMPLER_SETTINGS_KEY: &str = "sampler.settings";

/// Pads per deck in SAMPLER mode.
pub const SAMPLER_PADS: usize = 8;

/// Bins of the mini waveform of a slot.
pub const SAMPLER_PEAKS: usize = 48;

/// How a slot plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SamplerPlayMode {
    /// Plays to the end; a press restarts it.
    #[default]
    OneShot,
    /// Plays while the pad is held.
    Gate,
    /// Loops until stopped (SHIFT + pad); a press restarts it.
    Loop,
}

impl From<SamplerPlayMode> for SamplerMode {
    fn from(mode: SamplerPlayMode) -> Self {
        match mode {
            SamplerPlayMode::OneShot => SamplerMode::OneShot,
            SamplerPlayMode::Gate => SamplerMode::Gate,
            SamplerPlayMode::Loop => SamplerMode::Loop,
        }
    }
}

/// Which slots a deck's eight pads reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum SamplerLayout {
    /// Deck 1 reaches slots 1-8, deck 2 slots 9-16 (manual p.62).
    #[default]
    Split,
    /// Every deck reaches slots 1-8.
    Shared,
}

/// Per-slot settings the engine needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplerSlotParams {
    /// Linear gain of the slot.
    pub gain: Gain,
    /// Play mode.
    pub mode: SamplerPlayMode,
    /// Follow the master tempo (varispeed, pitch follows; see
    /// `EngineCommand::SetSamplerSync`).
    pub sync: bool,
}

/// The engine side of the sampler. Wire the engine's sampler API behind it.
///
/// Samples are interleaved stereo `f32` at the engine rate. Calls come from
/// the control side (never the audio thread) and may allocate.
pub trait SamplerEngine: Send + Sync {
    /// Hands a decoded sample to `slot`, replacing what it held. `bpm` is the
    /// analysed tempo of a library track, needed for sync.
    fn load_sample(
        &self,
        slot: SamplerSlotId,
        samples: Arc<[f32]>,
        bpm: Option<Bpm>,
    ) -> Result<(), CoreError>;
    /// Empties `slot`.
    fn clear_sample(&self, slot: SamplerSlotId) -> Result<(), CoreError>;
    /// Applies a slot's gain, play mode and sync.
    fn set_slot_params(
        &self,
        slot: SamplerSlotId,
        params: SamplerSlotParams,
    ) -> Result<(), CoreError>;
    /// Sends the sampler bus to the headphones cue.
    fn set_pfl(&self, on: bool) -> Result<(), CoreError>;
}

/// Engine stand-in: remembers the call, plays nothing.
#[derive(Debug, Default)]
pub struct StubSamplerEngine {
    log: Mutex<Vec<String>>,
}

impl StubSamplerEngine {
    /// Calls received so far, oldest first.
    pub fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn note(&self, line: String) {
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line);
    }
}

impl SamplerEngine for StubSamplerEngine {
    fn load_sample(
        &self,
        slot: SamplerSlotId,
        samples: Arc<[f32]>,
        bpm: Option<Bpm>,
    ) -> Result<(), CoreError> {
        self.note(format!(
            "load {} {} {:?}",
            slot.index(),
            samples.len(),
            bpm.map(|b| b.0)
        ));
        Ok(())
    }
    fn clear_sample(&self, slot: SamplerSlotId) -> Result<(), CoreError> {
        self.note(format!("clear {}", slot.index()));
        Ok(())
    }
    fn set_slot_params(
        &self,
        slot: SamplerSlotId,
        params: SamplerSlotParams,
    ) -> Result<(), CoreError> {
        self.note(format!(
            "params {} {:.2} {:?} {}",
            slot.index(),
            params.gain.0,
            params.mode,
            params.sync
        ));
        Ok(())
    }
    fn set_pfl(&self, on: bool) -> Result<(), CoreError> {
        self.note(format!("pfl {on}"));
        Ok(())
    }
}

/// The engine's sampler behind [`SamplerEngine`]. Samples go to the control
/// thread, which hands them to the engine and keeps them for a restart;
/// settings become `EngineCommand`s.
#[derive(Debug, Clone)]
pub struct EngineSampler {
    audio: AudioController,
}

impl EngineSampler {
    /// A hand-off through `audio`.
    pub fn new(audio: AudioController) -> Self {
        Self { audio }
    }
}

impl SamplerEngine for EngineSampler {
    fn load_sample(
        &self,
        slot: SamplerSlotId,
        samples: Arc<[f32]>,
        bpm: Option<Bpm>,
    ) -> Result<(), CoreError> {
        let buffer = TrackBuffer::new(samples).map_err(|e| CoreError::Media(e.to_string()))?;
        let bpm = bpm.filter(|b| b.is_valid());
        self.audio.post(Msg::SamplerLoad { slot, buffer, bpm })
    }

    fn clear_sample(&self, slot: SamplerSlotId) -> Result<(), CoreError> {
        self.audio.post(Msg::SamplerClear(slot))
    }

    fn set_slot_params(
        &self,
        slot: SamplerSlotId,
        params: SamplerSlotParams,
    ) -> Result<(), CoreError> {
        self.audio.send(EngineCommand::SetSamplerMode {
            slot,
            mode: params.mode.into(),
        })?;
        self.audio.send(EngineCommand::SetSamplerGain {
            slot,
            gain: params.gain,
        })?;
        self.audio.send(EngineCommand::SetSamplerSync {
            slot,
            on: params.sync,
        })
    }

    fn set_pfl(&self, on: bool) -> Result<(), CoreError> {
        self.audio.send(EngineCommand::SetSamplerPfl { on })
    }
}

/// One slot as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SamplerSlotDto {
    /// Slot, `0..SAMPLER_SLOTS`.
    pub slot: u8,
    /// Display name.
    pub name: String,
    /// Source file, `None` for an empty slot.
    pub path: Option<String>,
    /// Library track the file came from.
    pub track: Option<TrackId>,
    /// Colour index `0..8` (the hot cue palette).
    pub color: u8,
    /// Linear gain, `0..=2`.
    pub gain: f32,
    /// Play mode.
    pub play_mode: SamplerPlayMode,
    /// Start in sync with the leader.
    pub sync: bool,
    /// Decoded and handed to the engine.
    pub loaded: bool,
    /// Decoding.
    pub loading: bool,
    /// Length in seconds.
    pub duration_secs: f64,
    /// `SAMPLER_PEAKS` peak values `0..=1` for the mini waveform.
    pub peaks: Vec<f32>,
}

impl SamplerSlotDto {
    fn empty(slot: u8) -> Self {
        Self {
            slot,
            name: String::new(),
            path: None,
            track: None,
            color: slot % 8,
            gain: 1.0,
            play_mode: SamplerPlayMode::OneShot,
            sync: false,
            loaded: false,
            loading: false,
            duration_secs: 0.0,
            peaks: Vec::new(),
        }
    }
}

/// The whole sampler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SamplerStateDto {
    /// `SAMPLER_SLOTS` slots.
    pub slots: Vec<SamplerSlotDto>,
    /// Which slots each deck's pads reach.
    pub layout: SamplerLayout,
    /// Sampler bus level, linear `0..=1`.
    pub level: f32,
    /// Sampler bus in the headphones cue.
    pub pfl: bool,
}

/// What the UI may change on a slot; `None` keeps the value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SamplerSlotPatch {
    /// New name.
    pub name: Option<String>,
    /// New colour index.
    pub color: Option<u8>,
    /// New gain `0..=2`.
    pub gain: Option<f32>,
    /// New play mode.
    pub play_mode: Option<SamplerPlayMode>,
    /// New sync flag.
    pub sync: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSlot {
    slot: u8,
    name: String,
    path: String,
    track: Option<TrackId>,
    color: u8,
    gain: f32,
    play_mode: SamplerPlayMode,
    sync: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Stored {
    #[serde(default)]
    layout: SamplerLayout,
    #[serde(default = "full_level")]
    level: f32,
    #[serde(default)]
    pfl: bool,
    #[serde(default)]
    slots: Vec<StoredSlot>,
}

fn full_level() -> f32 {
    1.0
}

struct Inner {
    state: SamplerStateDto,
    /// Bumped per load so a stale decode is dropped.
    generation: [u64; SAMPLER_SLOTS],
    /// Engine rate each loaded slot was decoded at.
    rates: [Option<u32>; SAMPLER_SLOTS],
    engine: Arc<dyn SamplerEngine>,
    audio: Option<AudioController>,
    store: Option<Library>,
    library: Option<LibraryService>,
}

/// Shared sampler state and operations. Cloning shares the state.
#[derive(Clone)]
pub struct SamplerService {
    inner: Arc<Mutex<Inner>>,
    notifier: Notifier,
}

impl std::fmt::Debug for SamplerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamplerService").finish_non_exhaustive()
    }
}

fn slot_id(slot: u8) -> Result<SamplerSlotId, CoreError> {
    SamplerSlotId::new(slot).ok_or_else(|| CoreError::Invalid(format!("sampler slot {slot}")))
}

impl SamplerService {
    pub(crate) fn new(notifier: Notifier) -> Self {
        let state = SamplerStateDto {
            slots: (0..SAMPLER_SLOTS as u8)
                .map(SamplerSlotDto::empty)
                .collect(),
            layout: SamplerLayout::Split,
            level: 1.0,
            pfl: false,
        };
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state,
                generation: [0; SAMPLER_SLOTS],
                rates: [None; SAMPLER_SLOTS],
                engine: Arc::new(StubSamplerEngine::default()),
                audio: None,
                store: None,
                library: None,
            })),
            notifier,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the engine hand-off (the stub by default).
    pub fn set_engine(&self, engine: Arc<dyn SamplerEngine>) {
        self.lock().engine = engine;
    }

    /// Connects the sampler to the control thread and the library, restores
    /// the stored assignment and starts decoding it.
    pub fn attach(&self, audio: AudioController, library: Option<LibraryService>) {
        let store = library.as_ref().map(LibraryService::library);
        let stored = store.as_ref().and_then(|s| {
            s.get_setting_json::<Stored>(SAMPLER_SETTINGS_KEY)
                .ok()
                .flatten()
        });
        {
            let mut inner = self.lock();
            inner.audio = Some(audio.clone());
            inner.store = store;
            inner.library = library;
            if let Some(stored) = &stored {
                inner.state.layout = stored.layout;
                inner.state.level = stored.level.clamp(0.0, 1.0);
                inner.state.pfl = stored.pfl;
                for s in &stored.slots {
                    if let Some(slot) = inner.state.slots.get_mut(usize::from(s.slot)) {
                        slot.name.clone_from(&s.name);
                        slot.path = Some(s.path.clone());
                        slot.track = s.track;
                        slot.color = s.color % 8;
                        slot.gain = s.gain.clamp(0.0, 2.0);
                        slot.play_mode = s.play_mode;
                        slot.sync = s.sync;
                    }
                }
            }
        }
        if let Some(stored) = stored {
            let _ = audio.send(EngineCommand::SetSamplerLevel {
                gain: Gain(stored.level.clamp(0.0, 1.0)),
            });
            let _ = self.lock().engine.set_pfl(stored.pfl);
            for s in stored.slots {
                let _ = self.start_load(s.slot, PathBuf::from(s.path), s.track, false);
            }
        }
        self.publish();
    }

    /// Snapshot for the UI.
    pub fn state(&self) -> SamplerStateDto {
        self.lock().state.clone()
    }

    /// Loaded flag per slot, for pad LEDs.
    pub fn loaded_mask(&self) -> [bool; SAMPLER_SLOTS] {
        let inner = self.lock();
        std::array::from_fn(|i| inner.state.slots[i].loaded)
    }

    /// The slot `pad` of `deck` reaches under the current layout.
    pub fn slot_for_pad(&self, deck: DeckId, pad: u8) -> Option<SamplerSlotId> {
        let layout = self.lock().state.layout;
        slot_for(layout, deck, pad)
    }

    fn publish(&self) {
        let state = self.state();
        self.notifier.send(PadEvent::Sampler(Box::new(state)));
    }

    fn persist(&self) {
        let inner = self.lock();
        let Some(store) = &inner.store else { return };
        let stored = Stored {
            layout: inner.state.layout,
            level: inner.state.level,
            pfl: inner.state.pfl,
            slots: inner
                .state
                .slots
                .iter()
                .filter_map(|s| {
                    Some(StoredSlot {
                        slot: s.slot,
                        name: s.name.clone(),
                        path: s.path.clone()?,
                        track: s.track,
                        color: s.color,
                        gain: s.gain,
                        play_mode: s.play_mode,
                        sync: s.sync,
                    })
                })
                .collect(),
        };
        if let Err(err) = store.set_setting_json(SAMPLER_SETTINGS_KEY, &stored) {
            tracing::warn!(%err, "could not store the sampler");
        }
    }

    /// Loads a file into `slot`. Decoding runs on its own thread; the new
    /// state is published when it finishes.
    pub fn load_path(
        &self,
        slot: u8,
        path: &Path,
        track: Option<TrackId>,
    ) -> Result<(), CoreError> {
        self.start_load(slot, path.to_path_buf(), track, true)
    }

    /// Loads a library track into `slot`.
    pub fn load_track(&self, slot: u8, track: TrackId) -> Result<(), CoreError> {
        let library = self
            .lock()
            .library
            .clone()
            .ok_or_else(|| CoreError::Library("the library is not open".to_owned()))?;
        let detail = library.get_track(track)?;
        self.start_load(slot, PathBuf::from(detail.path), Some(track), true)
    }

    fn start_load(
        &self,
        slot: u8,
        path: PathBuf,
        track: Option<TrackId>,
        persist: bool,
    ) -> Result<(), CoreError> {
        slot_id(slot)?;
        let (generation, rate) = {
            let mut inner = self.lock();
            let i = usize::from(slot);
            inner.generation[i] += 1;
            let s = &mut inner.state.slots[i];
            let keep_name = s.path.as_ref().is_some_and(|p| Path::new(p) == path);
            if !keep_name || s.name.is_empty() {
                s.name = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
            s.path = Some(path.to_string_lossy().into_owned());
            s.track = track;
            s.loaded = false;
            s.loading = true;
            let rate = inner
                .audio
                .as_ref()
                .and_then(|a| a.status().ok())
                .and_then(|s| s.sample_rate)
                .unwrap_or(48_000);
            (inner.generation[i], rate)
        };
        self.publish();
        if persist {
            self.persist();
        }
        let service = self.clone();
        std::thread::Builder::new()
            .name("opendeck-sampler-load".into())
            .spawn(move || service.decode(slot, path, track, generation, SampleRate(rate)))
            .map_err(|e| CoreError::Media(e.to_string()))?;
        Ok(())
    }

    fn decode(
        &self,
        slot: u8,
        path: PathBuf,
        track: Option<TrackId>,
        generation: u64,
        rate: SampleRate,
    ) {
        let outcome = decode_file(&path, rate, &CancelFlag::new());
        // The analysed tempo lets a library sample follow the master (sync).
        let library = self.lock().library.clone();
        let bpm = track
            .zip(library)
            .and_then(|(track, library)| library.get_analysis(track).ok().flatten())
            .and_then(|analysis| analysis.bpm)
            .map(Bpm)
            .filter(|b| b.is_valid());
        let mut error = None;
        {
            let mut inner = self.lock();
            let i = usize::from(slot);
            if inner.generation[i] != generation {
                return;
            }
            match outcome {
                Ok(audio) => {
                    let id = slot_id(slot).ok();
                    let secs = audio.frames as f64 / f64::from(rate.0);
                    let peaks = peaks_of(&audio.samples);
                    let samples: Arc<[f32]> = audio.samples.into();
                    let handed = id.map(|id| inner.engine.load_sample(id, samples, bpm));
                    let params = {
                        let s = &inner.state.slots[i];
                        SamplerSlotParams {
                            gain: Gain(s.gain),
                            mode: s.play_mode,
                            sync: s.sync,
                        }
                    };
                    if let (Some(id), Some(Ok(()))) = (id, &handed) {
                        let _ = inner.engine.set_slot_params(id, params);
                    }
                    if matches!(handed, Some(Ok(()))) {
                        inner.rates[i] = Some(rate.0);
                    }
                    let s = &mut inner.state.slots[i];
                    s.loading = false;
                    match handed {
                        Some(Ok(())) => {
                            s.loaded = true;
                            s.duration_secs = secs;
                            s.peaks = peaks;
                        }
                        Some(Err(e)) => error = Some(e.to_string()),
                        None => {}
                    }
                }
                Err(e) => {
                    let s = &mut inner.state.slots[i];
                    s.loading = false;
                    s.loaded = false;
                    error = Some(e.to_string());
                }
            }
        }
        if let Some(error) = error {
            tracing::warn!(slot, %error, "sampler load failed");
        }
        self.publish();
    }

    /// Decodes again the slots that were decoded at another rate than the
    /// engine's. Call after the audio settings change: the engine keeps
    /// samples across a restart only when the rate is unchanged.
    pub fn follow_rate(&self) {
        let (audio, rates) = {
            let inner = self.lock();
            (inner.audio.clone(), inner.rates)
        };
        let Some(rate) = audio
            .and_then(|a| a.status().ok())
            .and_then(|s| s.sample_rate)
        else {
            return;
        };
        for (slot, decoded) in rates.iter().enumerate() {
            if decoded.is_none_or(|r| r == rate) {
                continue;
            }
            let (path, track) = {
                let inner = self.lock();
                let s = &inner.state.slots[slot];
                (s.path.clone(), s.track)
            };
            if let (Some(path), Ok(slot)) = (path, u8::try_from(slot)) {
                let _ = self.start_load(slot, PathBuf::from(path), track, false);
            }
        }
    }

    /// Empties a slot.
    pub fn clear(&self, slot: u8) -> Result<(), CoreError> {
        let id = slot_id(slot)?;
        {
            let mut inner = self.lock();
            let i = usize::from(slot);
            inner.generation[i] += 1;
            inner.rates[i] = None;
            inner.state.slots[i] = SamplerSlotDto::empty(slot);
            let _ = inner.engine.clear_sample(id);
        }
        self.publish();
        self.persist();
        Ok(())
    }

    /// Changes a slot's name, colour, gain, play mode or sync.
    pub fn patch(&self, slot: u8, patch: SamplerSlotPatch) -> Result<(), CoreError> {
        let id = slot_id(slot)?;
        if patch.gain.is_some_and(|g| !g.is_finite()) {
            return Err(CoreError::Invalid("gain is not a finite number".to_owned()));
        }
        {
            let mut inner = self.lock();
            let s = &mut inner.state.slots[usize::from(slot)];
            if let Some(name) = patch.name {
                s.name = name;
            }
            if let Some(color) = patch.color {
                s.color = color % 8;
            }
            if let Some(gain) = patch.gain {
                s.gain = gain.clamp(0.0, 2.0);
            }
            if let Some(mode) = patch.play_mode {
                s.play_mode = mode;
            }
            if let Some(sync) = patch.sync {
                s.sync = sync;
            }
            let params = SamplerSlotParams {
                gain: Gain(s.gain),
                mode: s.play_mode,
                sync: s.sync,
            };
            let _ = inner.engine.set_slot_params(id, params);
        }
        self.publish();
        self.persist();
        Ok(())
    }

    /// Chooses which slots each deck reaches.
    pub fn set_layout(&self, layout: SamplerLayout) {
        self.lock().state.layout = layout;
        self.publish();
        self.persist();
    }

    /// Sampler bus level, linear `0..=1`.
    pub fn set_level(&self, level: f32) -> Result<(), CoreError> {
        if !level.is_finite() {
            return Err(CoreError::Invalid(
                "level is not a finite number".to_owned(),
            ));
        }
        let level = level.clamp(0.0, 1.0);
        let audio = {
            let mut inner = self.lock();
            inner.state.level = level;
            inner.audio.clone()
        };
        if let Some(audio) = audio {
            audio.send(EngineCommand::SetSamplerLevel { gain: Gain(level) })?;
        }
        self.publish();
        self.persist();
        Ok(())
    }

    /// Sampler bus in the headphones cue.
    pub fn set_pfl(&self, on: bool) -> Result<(), CoreError> {
        let engine = {
            let mut inner = self.lock();
            inner.state.pfl = on;
            inner.engine.clone()
        };
        let result = engine.set_pfl(on);
        self.publish();
        self.persist();
        result
    }

    /// Pad edge on `slot`. A gate slot follows the pad; one-shot and loop
    /// slots act on the press only (the engine applies the slot's mode).
    pub fn trigger(&self, slot: SamplerSlotId, pressed: bool) -> Result<(), CoreError> {
        let (audio, mode) = {
            let inner = self.lock();
            (
                inner.audio.clone(),
                inner.state.slots[slot.index()].play_mode,
            )
        };
        let Some(audio) = audio else {
            return Err(CoreError::NotRunning);
        };
        if !pressed && mode != SamplerPlayMode::Gate {
            return Ok(());
        }
        audio.send(EngineCommand::SamplerTrigger { slot, pressed })
    }

    /// Stops a slot.
    pub fn stop(&self, slot: SamplerSlotId) -> Result<(), CoreError> {
        let audio = self.lock().audio.clone().ok_or(CoreError::NotRunning)?;
        audio.send(EngineCommand::SamplerStop { slot })
    }
}

pub(crate) fn slot_for(layout: SamplerLayout, deck: DeckId, pad: u8) -> Option<SamplerSlotId> {
    if usize::from(pad) >= SAMPLER_PADS {
        return None;
    }
    let base = match layout {
        SamplerLayout::Split => deck.index() * SAMPLER_PADS,
        SamplerLayout::Shared => 0,
    };
    SamplerSlotId::new(u8::try_from(base + usize::from(pad)).ok()?)
}

/// Max absolute value per bin of an interleaved stereo buffer.
fn peaks_of(samples: &[f32]) -> Vec<f32> {
    let frames = samples.len() / 2;
    if frames == 0 {
        return vec![0.0; SAMPLER_PEAKS];
    }
    (0..SAMPLER_PEAKS)
        .map(|bin| {
            let a = bin * frames / SAMPLER_PEAKS;
            let b = ((bin + 1) * frames / SAMPLER_PEAKS).max(a + 1).min(frames);
            samples[a * 2..b * 2]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()))
                .min(1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck(n: u8) -> DeckId {
        DeckId::new(n).expect("deck")
    }

    #[test]
    fn split_layout_gives_each_deck_eight_slots() {
        let s = |d, p| slot_for(SamplerLayout::Split, deck(d), p).map(SamplerSlotId::index);
        assert_eq!(s(0, 0), Some(0));
        assert_eq!(s(0, 7), Some(7));
        assert_eq!(s(1, 0), Some(8));
        assert_eq!(s(1, 7), Some(15));
        assert_eq!(s(0, 8), None);
    }

    #[test]
    fn shared_layout_maps_both_decks_to_the_first_bank() {
        let s = |d, p| slot_for(SamplerLayout::Shared, deck(d), p).map(SamplerSlotId::index);
        assert_eq!(s(0, 3), Some(3));
        assert_eq!(s(1, 3), Some(3));
    }

    #[test]
    fn peaks_cover_the_buffer() {
        let mut samples = vec![0.0f32; 4800];
        samples[2400] = 0.8;
        let peaks = peaks_of(&samples);
        assert_eq!(peaks.len(), SAMPLER_PEAKS);
        assert!((peaks.iter().cloned().fold(0.0, f32::max) - 0.8).abs() < 1e-6);
        assert_eq!(peaks_of(&[]), vec![0.0; SAMPLER_PEAKS]);
    }

    #[test]
    fn patch_clamps_and_reaches_the_engine() {
        let engine = Arc::new(StubSamplerEngine::default());
        let service = SamplerService::new(Notifier::default());
        service.set_engine(engine.clone());
        service
            .patch(
                2,
                SamplerSlotPatch {
                    gain: Some(5.0),
                    play_mode: Some(SamplerPlayMode::Loop),
                    name: Some("Air horn".into()),
                    ..SamplerSlotPatch::default()
                },
            )
            .expect("patch");
        let slot = &service.state().slots[2];
        assert_eq!(slot.gain, 2.0);
        assert_eq!(slot.name, "Air horn");
        assert!(
            engine
                .calls()
                .iter()
                .any(|c| c.starts_with("params 2 2.00 Loop"))
        );
        assert!(
            service
                .patch(
                    2,
                    SamplerSlotPatch {
                        gain: Some(f32::NAN),
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert!(service.patch(16, SamplerSlotPatch::default()).is_err());
    }
}
