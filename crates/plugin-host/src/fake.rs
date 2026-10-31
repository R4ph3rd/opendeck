//! An in-process fake plugin format for tests and CI (provider `"fake"`).
//! app-core never registers it in the product.
//!
//! Files with the `.fakeplug` extension are "plugins". Scanning one lists
//! two plugins, `fake.gain` (effect) and `fake.sine` (instrument), unless
//! the file stem asks for a failure, which the scanner tests use:
//!
//! - `crash`: the scanner process aborts;
//! - `hang`: the scan never returns (the host kills it at the timeout);
//! - `broken`: the scan returns a load error.
//!
//! `fake.gain`: one parameter, id 0, "Gain", 0 to 2, default 1, linear;
//! output = input x gain. State is the gain as 4 little-endian bytes.
//! `fake.sine`: a monophonic sine; note-on sets frequency
//! `440 * 2^((key - 69) / 12)` Hz and amplitude = velocity; note-off stops.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use opendeck_contracts::SampleRate;
use opendeck_contracts::fx::{EffectKey, ParamCurve, ParamDescriptor, ParamId, ParamUnit};
use opendeck_contracts::instrument::NoteKind;
use opendeck_contracts::plugin::{PluginInfo, PluginKind};

use crate::PluginError;
use crate::backend::{HostWake, PluginBackend, PluginInstance, PluginProcessor, ProcessBlock};

/// Provider id of the fake format.
pub const FAKE_PROVIDER: &str = "fake";
/// Id of the fake gain effect.
pub const FAKE_GAIN: &str = "fake.gain";
/// Id of the fake sine instrument.
pub const FAKE_SINE: &str = "fake.sine";

/// The fake format.
#[derive(Debug, Default, Clone, Copy)]
pub struct FakeBackend;

impl FakeBackend {
    /// What a scan of `path` lists (no failure modes). Lets tests fill a
    /// catalog without the scanner.
    pub fn plugins_in(path: &Path) -> Vec<PluginInfo> {
        let make = |id: &str, name: &str, kind| PluginInfo {
            key: EffectKey {
                provider: FAKE_PROVIDER.into(),
                id: id.into(),
            },
            kind,
            name: name.into(),
            vendor: "OpenDeck".into(),
            version: "1.0.0".into(),
            path: path.display().to_string(),
            categories: Vec::new(),
            has_editor: false,
        };
        vec![
            make(FAKE_GAIN, "Fake Gain", PluginKind::Effect),
            make(FAKE_SINE, "Fake Sine", PluginKind::Instrument),
        ]
    }
}

impl PluginBackend for FakeBackend {
    fn provider(&self) -> &'static str {
        FAKE_PROVIDER
    }

    fn extension(&self) -> &'static str {
        "fakeplug"
    }

    fn default_folders(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn scan_file(&self, path: &Path) -> Result<Vec<PluginInfo>, PluginError> {
        match path.file_stem().and_then(|s| s.to_str()) {
            Some("crash") => std::process::abort(),
            Some("hang") => loop {
                std::thread::park();
            },
            Some("broken") => Err(PluginError::Load {
                path: path.display().to_string(),
                reason: "no entry point".into(),
            }),
            _ => Ok(Self::plugins_in(path)),
        }
    }

    fn instantiate(
        &self,
        info: &PluginInfo,
        _wake: HostWake,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        match info.key.id.as_str() {
            FAKE_GAIN => Ok(Box::new(GainInstance {
                gain: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
                active: false,
            })),
            FAKE_SINE => Ok(Box::new(SineInstance { active: false })),
            _ => Err(PluginError::UnknownPlugin {
                provider: FAKE_PROVIDER.into(),
                id: info.key.id.clone(),
            }),
        }
    }
}

fn gain_param() -> ParamDescriptor {
    ParamDescriptor {
        id: ParamId(0),
        name: "Gain".into(),
        unit: ParamUnit::None,
        min: 0.0,
        max: 2.0,
        default: 1.0,
        curve: ParamCurve::Linear,
    }
}

struct GainInstance {
    /// Plain gain as f32 bits, shared with the processor (the way real
    /// plugins share parameter state between threads).
    gain: Arc<AtomicU32>,
    active: bool,
}

impl PluginInstance for GainInstance {
    fn params(&self) -> Vec<ParamDescriptor> {
        vec![gain_param()]
    }

    fn activate(
        &mut self,
        _: SampleRate,
        _: usize,
    ) -> Result<Box<dyn PluginProcessor>, PluginError> {
        if self.active {
            return Err(PluginError::Plugin("already active".into()));
        }
        self.active = true;
        Ok(Box::new(GainProcessor {
            gain: self.gain.clone(),
            desc: gain_param(),
        }))
    }

    fn deactivate(&mut self, _: Box<dyn PluginProcessor>) {
        self.active = false;
    }

    fn latency_frames(&self) -> u32 {
        0
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        Ok(self.gain.load(Ordering::Relaxed).to_le_bytes().to_vec())
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        let bytes: [u8; 4] = state
            .try_into()
            .map_err(|_| PluginError::Plugin("bad state size".into()))?;
        self.gain
            .store(u32::from_le_bytes(bytes), Ordering::Relaxed);
        Ok(())
    }
}

struct GainProcessor {
    gain: Arc<AtomicU32>,
    desc: ParamDescriptor,
}

impl PluginProcessor for GainProcessor {
    fn process(&mut self, mut block: ProcessBlock<'_>) {
        for c in block.params.iter().filter(|c| c.id == ParamId(0)) {
            let plain = self.desc.to_plain(c.normalized);
            self.gain.store(plain.to_bits(), Ordering::Relaxed);
        }
        let g = f32::from_bits(self.gain.load(Ordering::Relaxed));
        let (l, r) = block.output.channels();
        match block.input {
            Some((il, ir)) => {
                for (o, i) in l.iter_mut().zip(il).chain(r.iter_mut().zip(ir)) {
                    *o = *i * g;
                }
            }
            None => {
                l.fill(0.0);
                r.fill(0.0);
            }
        }
    }

    fn reset(&mut self) {}
}

struct SineInstance {
    active: bool,
}

impl PluginInstance for SineInstance {
    fn params(&self) -> Vec<ParamDescriptor> {
        Vec::new()
    }

    fn activate(
        &mut self,
        sample_rate: SampleRate,
        _: usize,
    ) -> Result<Box<dyn PluginProcessor>, PluginError> {
        if self.active {
            return Err(PluginError::Plugin("already active".into()));
        }
        self.active = true;
        Ok(Box::new(SineProcessor {
            rate: sample_rate.0 as f32,
            phase: 0.0,
            freq: 0.0,
            amp: 0.0,
            key: None,
        }))
    }

    fn deactivate(&mut self, _: Box<dyn PluginProcessor>) {
        self.active = false;
    }

    fn latency_frames(&self) -> u32 {
        0
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        Ok(Vec::new())
    }

    fn load_state(&mut self, _: &[u8]) -> Result<(), PluginError> {
        Ok(())
    }
}

struct SineProcessor {
    rate: f32,
    phase: f32,
    freq: f32,
    amp: f32,
    key: Option<u8>,
}

impl PluginProcessor for SineProcessor {
    fn process(&mut self, mut block: ProcessBlock<'_>) {
        let notes = block.notes;
        let (l, r) = block.output.channels();
        let mut next = notes.iter().peekable();
        for (i, (a, b)) in l.iter_mut().zip(r.iter_mut()).enumerate() {
            while let Some(n) = next.next_if(|n| n.offset() as usize <= i) {
                match n.kind() {
                    NoteKind::On => {
                        self.key = Some(n.key());
                        self.freq = 440.0 * 2.0_f32.powf((f32::from(n.key()) - 69.0) / 12.0);
                        self.amp = n.velocity();
                    }
                    NoteKind::Off if self.key == Some(n.key()) => {
                        self.key = None;
                        self.amp = 0.0;
                    }
                    NoteKind::Off => {}
                }
            }
            let s = self.amp * (self.phase * TAU).sin();
            *a = s;
            *b = s;
            self.phase = (self.phase + self.freq / self.rate).fract();
        }
    }

    fn reset(&mut self) {
        self.key = None;
        self.amp = 0.0;
        self.phase = 0.0;
    }
}
