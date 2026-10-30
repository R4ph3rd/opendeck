//! FX control model on top of the engine's FX units (milestone 5, ADR 0006).
//!
//! Runs on the control thread only. It owns the [`EffectRegistry`], builds and
//! prepares effects off the audio thread and installs them through
//! [`EngineHandle`]. Swapping an effect disables the slot, waits
//! [`SWAP_WAIT`] (the 10 ms bypass crossfade), then installs the new instance.
//!
//! # Engine slot layout (project choice)
//!
//! A deck's chain has `FX_SLOTS_PER_UNIT` (3) slots, and several features need
//! one. The layout, per deck:
//!
//! - slots 0 and 1: Beat FX effects of the unit assigned to that channel;
//! - slot 2 (the *utility slot*): Beat FX effect 3 when a Multi unit uses it,
//!   otherwise Sound Color FX, which pad FX and Release FX take over for a
//!   moment (the Sound Color FX returns when they finish).
//!
//! A Multi unit with a third effect therefore blocks Sound Color FX, pad FX and
//! Release FX on its decks ([`FxStateDto::utility_blocked`]). Raising
//! `FX_SLOTS_PER_UNIT` (a contract change) would lift the limit.
//!
//! Beat FX units are assigned to channel 1, 2 or 1&2. A channel has one chain,
//! so assigning a unit to a channel takes it from the other unit. 1&2 mirrors
//! every command to both deck chains and points both at the same tempo source.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use opendeck_contracts::fx::EffectRegistry;
use opendeck_contracts::fx::{EffectKey, FxPreset, ParamId};
use opendeck_contracts::ids::{FX_SEND_BUSES, FX_SLOTS_PER_UNIT, FxSlotId, FxUnit};
use opendeck_contracts::units::{Beats, Bpm, Frames, Gain};
use opendeck_contracts::{DeckId, EngineCommand, EngineTelemetry, MAX_DECKS, MVP_DECKS};
use opendeck_dsp_fx::catalog::{self, MIN_BPM};
use opendeck_dsp_fx::preset::{all_factory_presets, factory_presets};
use opendeck_dsp_fx::{BuiltinFactory, PROVIDER_ID};
use opendeck_engine::{EngineHandle, FxTempoSource, SendTap};
use opendeck_library::{Library, StoredFxPreset};

use crate::fx_types::{
    BEAT_FX_UNITS, BeatFxUnitDto, CfxDto, FxAssign, FxBpmSource, FxChainPresetDto, FxChainSlotDto,
    FxEffectDto, FxMode, FxOp, FxPresetsDto, FxSlotDto, FxSlotRef, FxStateDto, FxTarget, FxUnitRef,
    PAD_FX_COUNT, PadFxAssignDto, SendUnitDto,
};
use crate::types::CoreError;

/// Wait between disabling a slot and swapping its effect (the bypass crossfade).
pub(crate) const SWAP_WAIT: Duration = Duration::from_millis(10);
/// Utility slot index (see the module docs).
const UTIL: u8 = (FX_SLOTS_PER_UNIT - 1) as u8;
/// Settings key of the persisted [`FxStateDto`].
pub(crate) const STATE_KEY: &str = "fx.state";
/// Settings key of the user chain presets.
pub(crate) const CHAINS_KEY: &str = "fx.chains";
/// Persist this long after the last change (knob drags write often).
const PERSIST_DELAY: Duration = Duration::from_millis(600);
/// CFX knob values closer to the centre than this are neutral (effect off).
const CFX_DEAD_ZONE: f32 = 0.02;
/// A pad effect keeps the utility slot this long after release, for tails.
const PAD_TAIL: Duration = Duration::from_millis(400);
/// Same for effects with a tail (echo, reverb).
const PAD_TAIL_LONG: Duration = Duration::from_millis(3000);
/// Taps further apart than this start a new tap sequence, seconds.
const TAP_RESET_SECS: f64 = 2.0;
/// Taps closer than this are ignored (switch bounce), seconds.
const TAP_MIN_GAP_SECS: f64 = 0.15;
/// Taps kept for the average.
const TAP_WINDOW: usize = 8;
/// Highest tapped tempo.
const MAX_TAP_BPM: f64 = 300.0;

/// Beat divisions the BEAT left/right stepper walks, in beats. Project
/// choice. TODO(verify) against the rekordbox beat list.
pub(crate) const DIVISION_STEPS: [f64; 10] =
    [0.0625, 0.125, 0.25, 0.5, 0.75, 1.0, 2.0, 4.0, 8.0, 16.0];

/// Sound Color FX types offered per channel.
pub(crate) const CFX_KINDS: [&str; 3] = ["colour_filter", "noise", "bit_crusher"];

/// Default pad effects (project choice): `(effect id, division, release)`,
/// index `bank * 16 + shift * 8 + pad`.
pub(crate) const DEFAULT_PADS: [(&str, f64, bool); PAD_FX_COUNT] = [
    // Pad FX 1
    ("echo", 0.25, false),
    ("echo", 0.5, false),
    ("echo", 1.0, false),
    ("ping_pong", 0.5, false),
    ("delay", 0.125, false),
    ("delay", 0.25, false),
    ("reverb", 1.0, false),
    ("spiral", 1.0, false),
    // Pad FX 1, shift
    ("roll", 0.125, false),
    ("roll", 0.25, false),
    ("roll", 0.5, false),
    ("reverse_roll", 0.25, false),
    ("reverse_roll", 0.5, false),
    ("helix", 1.0, false),
    ("trans", 0.25, false),
    ("trans", 0.5, false),
    // Pad FX 2
    ("flanger", 1.0, false),
    ("flanger", 4.0, false),
    ("phaser", 1.0, false),
    ("phaser", 4.0, false),
    ("filter", 1.0, false),
    ("filter", 4.0, false),
    ("noise", 1.0, false),
    ("bit_crusher", 1.0, false),
    // Pad FX 2, shift
    ("pitch", 1.0, false),
    ("pitch", 2.0, false),
    ("vinyl_brake", 0.5, true),
    ("vinyl_brake", 1.0, true),
    ("vinyl_brake", 2.0, true),
    ("echo", 2.0, true),
    ("reverb", 2.0, true),
    ("spiral", 2.0, true),
];

fn builtin(id: &str) -> EffectKey {
    EffectKey {
        provider: PROVIDER_ID.to_owned(),
        id: id.to_owned(),
    }
}

/// Everything the controller needs from the control thread for one call.
pub(crate) struct FxEnv<'a> {
    /// The open engine, if any.
    pub handle: Option<&'a mut EngineHandle>,
    /// Newest telemetry snapshot.
    pub tel: Option<&'a EngineTelemetry>,
    /// Quantize flag per deck.
    pub quantize: [bool; MAX_DECKS],
    /// Sync leader.
    pub leader: Option<DeckId>,
    /// Library for presets and persistence.
    pub library: Option<&'a Library>,
    /// Current time.
    pub now: Instant,
}

/// Engine slot the controller drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Addr {
    Deck(DeckId, u8),
    Send(u8, u8),
}

impl Addr {
    fn id(self) -> Option<FxSlotId> {
        match self {
            Addr::Deck(d, s) => FxSlotId::new(FxUnit::Deck(d), s),
            Addr::Send(b, s) => FxSlotId::new(FxUnit::Send(b), s),
        }
    }
}

/// What to do once a swapped-in effect is installed.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Then {
    Nothing,
    /// Engage the utility slot at a frame.
    Engage(Frames),
    /// Release FX: `from` (a Beat FX slot) hands over to the utility slot.
    Release {
        from: Option<Addr>,
        hold: Frames,
    },
}

struct Pending {
    addr: Addr,
    ready: Instant,
    then: Then,
}

/// A pad or release effect that has the utility slot.
#[derive(Debug, Clone)]
struct Override {
    key: EffectKey,
    params: Vec<f32>,
    wet: f32,
    division: f64,
    /// Pad index while held; `None` for a release.
    held_by: Option<u8>,
    /// When the slot goes back to the Sound Color FX; `None` while held.
    expires: Option<Instant>,
}

/// What an engine slot should hold.
#[derive(Debug, Clone)]
struct Desired {
    key: EffectKey,
    params: Vec<f32>,
    wet: f32,
    division: f64,
    /// `Some` when the controller drives the enabled flag; pad and release
    /// effects are engaged by timed events instead.
    on: Option<bool>,
}

/// Control-thread FX model.
pub(crate) struct FxController {
    registry: opendeck_contracts::fx::EffectRegistry,
    effects: HashMap<EffectKey, FxEffectDto>,
    state: FxStateDto,
    chains: Vec<FxChainPresetDto>,
    user_presets: Vec<FxPreset>,
    installed_deck: [[Option<EffectKey>; FX_SLOTS_PER_UNIT]; MAX_DECKS],
    installed_send: [[Option<EffectKey>; FX_SLOTS_PER_UNIT]; FX_SEND_BUSES],
    pending: Vec<Pending>,
    overrides: [Option<Override>; MAX_DECKS],
    taps: [Vec<f64>; BEAT_FX_UNITS],
    epoch: Instant,
    last_leader: Option<DeckId>,
    dirty: bool,
    persist_at: Option<Instant>,
}

// ---------- pure helpers ----------

/// Tempo from tap times in seconds: 60 over the mean interval, clamped to
/// `MIN_BPM..=300`. `None` with fewer than two taps.
pub(crate) fn tap_bpm(taps: &[f64]) -> Option<f64> {
    if taps.len() < 2 {
        return None;
    }
    let span = taps[taps.len() - 1] - taps[0];
    let mean = span / (taps.len() - 1) as f64;
    (mean > 0.0).then(|| (60.0 / mean).clamp(MIN_BPM, MAX_TAP_BPM))
}

/// Adds a tap at `t` seconds. Starts over after a long gap, ignores bounces.
pub(crate) fn push_tap(taps: &mut Vec<f64>, t: f64) {
    if let Some(&last) = taps.last() {
        if t - last < TAP_MIN_GAP_SECS {
            return;
        }
        if t - last > TAP_RESET_SECS {
            taps.clear();
        }
    }
    taps.push(t);
    if taps.len() > TAP_WINDOW {
        taps.remove(0);
    }
}

/// Divisions an effect accepts, ascending, from [`DIVISION_STEPS`] limited to
/// its catalog range. The factory default is always included.
pub(crate) fn divisions_of(key: &EffectKey) -> Vec<f64> {
    if key.provider != PROVIDER_ID {
        return Vec::new();
    }
    let Some(sync) = catalog::find(&key.id).and_then(|s| s.sync) else {
        return Vec::new();
    };
    let mut list: Vec<f64> = DIVISION_STEPS
        .iter()
        .copied()
        .filter(|d| *d >= sync.min_beats - 1e-9 && *d <= sync.max_beats + 1e-9)
        .collect();
    if !list.iter().any(|d| (d - sync.default_beats).abs() < 1e-9) {
        list.push(sync.default_beats);
        list.sort_by(f64::total_cmp);
    }
    list
}

/// Index of the entry nearest to `value` (log distance).
fn nearest(list: &[f64], value: f64) -> usize {
    let v = value.max(1e-6).ln();
    list.iter()
        .enumerate()
        .min_by(|a, b| (a.1.ln() - v).abs().total_cmp(&(b.1.ln() - v).abs()))
        .map_or(0, |(i, _)| i)
}

fn decks_of(assign: FxAssign) -> [bool; MVP_DECKS] {
    match assign {
        FxAssign::None => [false, false],
        FxAssign::Ch1 => [true, false],
        FxAssign::Ch2 => [false, true],
        FxAssign::Both => [true, true],
    }
}

fn assign_of(decks: [bool; MVP_DECKS]) -> FxAssign {
    match decks {
        [true, true] => FxAssign::Both,
        [true, false] => FxAssign::Ch1,
        [false, true] => FxAssign::Ch2,
        _ => FxAssign::None,
    }
}

/// Parameters, wet and on-state of a Sound Color FX at knob position `amount`.
/// Colour Filter is bipolar on its `colour` parameter; the other types scale
/// their primary parameter and the wet amount with the distance from the
/// centre.
pub(crate) fn cfx_mapping(info: &FxEffectDto, amount: f32) -> (Vec<f32>, f32, bool) {
    let amount = amount.clamp(-1.0, 1.0);
    let mut params: Vec<f32> = info.params.iter().map(|p| p.default_normalized()).collect();
    let on = amount.abs() > CFX_DEAD_ZONE;
    let magnitude = amount.abs();
    let mut wet = 1.0;
    if info.key.id == "colour_filter" {
        if let Some(p) = params.first_mut() {
            *p = (amount + 1.0) / 2.0;
        }
    } else {
        wet = magnitude;
        if let Some(i) = info
            .primary
            .map(|p| p.0 as usize)
            .filter(|i| *i < params.len())
        {
            // Fewer bits is more crush: the bit depth runs against the knob.
            params[i] = if info.key.id == "bit_crusher" {
                1.0 - magnitude
            } else {
                magnitude
            };
        }
    }
    (params, wet, on)
}

fn effect_dtos(registry: &EffectRegistry) -> HashMap<EffectKey, FxEffectDto> {
    registry
        .effects()
        .into_iter()
        .map(|info| {
            let spec = (info.key.provider == PROVIDER_ID)
                .then(|| catalog::find(&info.key.id))
                .flatten();
            let primary = spec
                .and_then(|s| s.index_of(s.primary))
                .map(|i| ParamId(i as u32));
            let dto = FxEffectDto {
                key: info.key.clone(),
                name: info.name.clone(),
                routing: info.routing,
                beat_synced: info.beat_synced,
                has_tail: info.has_tail,
                params: info.params.clone(),
                primary,
                release: spec.is_some_and(|s| s.release),
                divisions: divisions_of(&info.key),
            };
            (info.key, dto)
        })
        .collect()
}

fn to_stored(preset: &FxPreset) -> StoredFxPreset {
    StoredFxPreset {
        provider: preset.effect.provider.clone(),
        effect_id: preset.effect.id.clone(),
        name: preset.name.clone(),
        params: serde_json::to_string(&preset.params).unwrap_or_else(|_| "[]".to_owned()),
        wet: f64::from(preset.wet),
        beat_division: preset.beat_division.0,
    }
}

fn from_stored(stored: &StoredFxPreset) -> Option<FxPreset> {
    Some(FxPreset {
        effect: EffectKey {
            provider: stored.provider.clone(),
            id: stored.effect_id.clone(),
        },
        name: stored.name.clone(),
        params: serde_json::from_str(&stored.params).ok()?,
        wet: stored.wet as f32,
        beat_division: Beats(stored.beat_division),
    })
}

fn invalid(msg: impl Into<String>) -> CoreError {
    CoreError::Invalid(msg.into())
}

impl FxController {
    /// Registers the built-in factory and loads stored state.
    pub(crate) fn new(library: Option<&Library>) -> Self {
        let mut registry = EffectRegistry::new();
        if let Err(err) = registry.register(Box::new(BuiltinFactory::new())) {
            tracing::error!(%err, "could not register the built-in effects");
        }
        let effects = effect_dtos(&registry);
        let mut controller = Self {
            registry,
            effects,
            state: FxStateDto {
                units: Vec::new(),
                cfx: Vec::new(),
                cfx_kinds: CFX_KINDS.iter().map(|id| builtin(id)).collect(),
                sends: Vec::new(),
                send_levels: Vec::new(),
                pads: Vec::new(),
                utility_blocked: vec![false; MVP_DECKS],
            },
            chains: Vec::new(),
            user_presets: Vec::new(),
            installed_deck: Default::default(),
            installed_send: Default::default(),
            pending: Vec::new(),
            overrides: Default::default(),
            taps: Default::default(),
            epoch: Instant::now(),
            last_leader: None,
            dirty: false,
            persist_at: None,
        };
        controller.state = controller.default_state();
        if let Some(lib) = library {
            controller.restore(lib);
        }
        controller.normalize();
        controller
    }

    // ----- state construction and restore -----

    fn default_slot(&self, key: &EffectKey, send: bool) -> FxSlotDto {
        let info = self.effects.get(key);
        let first = factory_presets(&key.id).into_iter().next();
        FxSlotDto {
            effect: Some(key.clone()),
            params: info
                .map(|i| i.params.iter().map(|p| p.default_normalized()).collect())
                .unwrap_or_default(),
            wet: if send {
                1.0
            } else {
                first.as_ref().map_or(1.0, |p| p.wet)
            },
            division: catalog::find(&key.id)
                .and_then(|s| s.sync)
                .map_or(1.0, |s| s.default_beats),
            enabled: true,
        }
    }

    fn default_state(&self) -> FxStateDto {
        let slots = |first: Option<(&str, bool)>| -> Vec<FxSlotDto> {
            let mut v = vec![FxSlotDto::empty(true); FX_SLOTS_PER_UNIT];
            if let Some((id, send)) = first {
                v[0] = self.default_slot(&builtin(id), send);
            }
            v
        };
        let unit = |assign, id| BeatFxUnitDto {
            assign,
            mode: FxMode::Multi,
            slots: slots(Some((id, false))),
            on: false,
            bpm_source: FxBpmSource::Auto,
            tap_bpm: None,
            release_effect: Some(builtin("vinyl_brake")),
            release_hold_ms: 2000,
        };
        FxStateDto {
            units: vec![unit(FxAssign::Ch1, "echo"), unit(FxAssign::Ch2, "flanger")],
            cfx: (0..MVP_DECKS)
                .map(|_| CfxDto {
                    kind: builtin("colour_filter"),
                    amount: 0.0,
                })
                .collect(),
            cfx_kinds: CFX_KINDS.iter().map(|id| builtin(id)).collect(),
            sends: vec![
                SendUnitDto {
                    slots: slots(Some(("reverb", true))),
                    post_fader: true,
                },
                SendUnitDto {
                    slots: slots(Some(("delay", true))),
                    post_fader: true,
                },
            ],
            send_levels: vec![vec![0.0; FX_SEND_BUSES]; MVP_DECKS],
            pads: DEFAULT_PADS
                .iter()
                .map(|(id, division, release)| PadFxAssignDto {
                    effect: Some(builtin(id)),
                    division: *division,
                    release: *release,
                })
                .collect(),
            utility_blocked: vec![false; MVP_DECKS],
        }
    }

    fn restore(&mut self, lib: &Library) {
        match lib.get_setting_json::<FxStateDto>(STATE_KEY) {
            Ok(Some(state)) => self.state = state,
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "stored FX state is unreadable, using defaults"),
        }
        match lib.get_setting_json::<Vec<FxChainPresetDto>>(CHAINS_KEY) {
            Ok(Some(chains)) => self.chains = chains,
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "stored FX chains are unreadable"),
        }
        match lib.list_fx_presets() {
            Ok(rows) => self.user_presets = rows.iter().filter_map(from_stored).collect(),
            Err(err) => tracing::warn!(%err, "could not read FX presets"),
        }
    }

    /// Repairs a stored or edited state: sizes, unknown effects, parameter
    /// counts, ranges, channel ownership.
    fn normalize(&mut self) {
        let defaults = self.default_state();
        let st = &mut self.state;
        st.units.truncate(BEAT_FX_UNITS);
        while st.units.len() < BEAT_FX_UNITS {
            st.units.push(defaults.units[st.units.len()].clone());
        }
        st.sends.truncate(FX_SEND_BUSES);
        while st.sends.len() < FX_SEND_BUSES {
            st.sends.push(defaults.sends[st.sends.len()].clone());
        }
        st.cfx.truncate(MVP_DECKS);
        while st.cfx.len() < MVP_DECKS {
            st.cfx.push(defaults.cfx[st.cfx.len()].clone());
        }
        st.pads.truncate(PAD_FX_COUNT);
        while st.pads.len() < PAD_FX_COUNT {
            st.pads.push(defaults.pads[st.pads.len()].clone());
        }
        st.cfx_kinds = defaults.cfx_kinds.clone();
        st.send_levels.truncate(MVP_DECKS);
        while st.send_levels.len() < MVP_DECKS {
            st.send_levels.push(vec![0.0; FX_SEND_BUSES]);
        }
        for row in &mut st.send_levels {
            row.resize(FX_SEND_BUSES, 0.0);
            for v in row.iter_mut() {
                *v = if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.0
                };
            }
        }
        let effects = &self.effects;
        let fix_slots = |slots: &mut Vec<FxSlotDto>| {
            slots.truncate(FX_SLOTS_PER_UNIT);
            while slots.len() < FX_SLOTS_PER_UNIT {
                slots.push(FxSlotDto::empty(true));
            }
            for slot in slots {
                match slot.effect.as_ref().and_then(|k| effects.get(k)) {
                    None => *slot = FxSlotDto::empty(slot.enabled),
                    Some(info) => {
                        let mut params: Vec<f32> =
                            info.params.iter().map(|p| p.default_normalized()).collect();
                        for (dst, src) in params.iter_mut().zip(&slot.params) {
                            if src.is_finite() {
                                *dst = src.clamp(0.0, 1.0);
                            }
                        }
                        slot.params = params;
                        slot.wet = if slot.wet.is_finite() {
                            slot.wet.clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        if !(slot.division.is_finite() && slot.division > 0.0) {
                            slot.division = 1.0;
                        }
                    }
                }
            }
        };
        for unit in &mut st.units {
            fix_slots(&mut unit.slots);
            unit.release_hold_ms = unit.release_hold_ms.clamp(50, 10_000);
            if unit
                .release_effect
                .as_ref()
                .is_some_and(|k| !effects.contains_key(k))
            {
                unit.release_effect = None;
            }
        }
        for send in &mut st.sends {
            fix_slots(&mut send.slots);
        }
        for cfx in &mut st.cfx {
            if !st.cfx_kinds.contains(&cfx.kind) {
                cfx.kind = builtin("colour_filter");
            }
            cfx.amount = if cfx.amount.is_finite() {
                cfx.amount.clamp(-1.0, 1.0)
            } else {
                0.0
            };
        }
        for pad in &mut st.pads {
            if pad
                .effect
                .as_ref()
                .is_some_and(|k| !effects.contains_key(k))
            {
                pad.effect = None;
            }
            if !(pad.division.is_finite() && pad.division > 0.0) {
                pad.division = 1.0;
            }
        }
        // One owner per channel: the lower unit keeps a contested channel.
        let mut taken = [false; MVP_DECKS];
        for unit in &mut st.units {
            let mut decks = decks_of(unit.assign);
            for (d, t) in decks.iter_mut().zip(&mut taken) {
                if *d && *t {
                    *d = false;
                }
                *t |= *d;
            }
            unit.assign = assign_of(decks);
        }
        self.refresh_blocked();
    }

    fn refresh_blocked(&mut self) {
        for d in 0..MVP_DECKS {
            self.state.utility_blocked[d] =
                self.owner(d).is_some_and(|u| self.slot_in_use(u, UTIL));
        }
    }

    // ----- queries -----

    /// Current state.
    pub(crate) fn state(&self) -> &FxStateDto {
        &self.state
    }

    /// Every effect the registry offers.
    pub(crate) fn effects(&self) -> Vec<FxEffectDto> {
        let mut list: Vec<FxEffectDto> = self.effects.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Factory presets, user presets and chains.
    pub(crate) fn presets(&self) -> FxPresetsDto {
        FxPresetsDto {
            factory: all_factory_presets(),
            user: self.user_presets.clone(),
            chains: self.chains.clone(),
        }
    }

    /// True once after a change the UI has not seen.
    pub(crate) fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn owner(&self, deck: usize) -> Option<usize> {
        self.state
            .units
            .iter()
            .position(|u| decks_of(u.assign).get(deck).copied().unwrap_or(false))
    }

    fn slot_in_use(&self, unit: usize, slot: u8) -> bool {
        let u = &self.state.units[unit];
        let s = &u.slots[usize::from(slot)];
        s.effect.is_some() && (u.mode == FxMode::Multi || slot == 0)
    }

    fn unit_decks(&self, unit: usize) -> Vec<DeckId> {
        let flags = decks_of(self.state.units[unit].assign);
        (0..MVP_DECKS)
            .filter(|i| flags[*i])
            .filter_map(|i| DeckId::new(i as u8))
            .collect()
    }

    fn installed(&mut self, a: Addr) -> &mut Option<EffectKey> {
        match a {
            Addr::Deck(d, s) => &mut self.installed_deck[d.index()][usize::from(s)],
            Addr::Send(b, s) => &mut self.installed_send[usize::from(b)][usize::from(s)],
        }
    }

    fn desired(&self, a: Addr) -> Option<Desired> {
        match a {
            Addr::Send(bus, slot) => {
                let s = &self.state.sends[usize::from(bus)].slots[usize::from(slot)];
                Some(Desired {
                    key: s.effect.clone()?,
                    params: s.params.clone(),
                    wet: s.wet,
                    division: s.division,
                    on: Some(s.enabled),
                })
            }
            Addr::Deck(deck, slot) => {
                let d = deck.index();
                if slot == UTIL
                    && let Some(o) = &self.overrides[d]
                {
                    return Some(Desired {
                        key: o.key.clone(),
                        params: o.params.clone(),
                        wet: o.wet,
                        division: o.division,
                        on: None,
                    });
                }
                if let Some(u) = self.owner(d)
                    && self.slot_in_use(u, slot)
                {
                    let unit = &self.state.units[u];
                    let s = &unit.slots[usize::from(slot)];
                    return Some(Desired {
                        key: s.effect.clone()?,
                        params: s.params.clone(),
                        wet: s.wet,
                        division: s.division,
                        on: Some(unit.on && s.enabled),
                    });
                }
                if slot == UTIL {
                    let cfx = &self.state.cfx[d];
                    let info = self.effects.get(&cfx.kind)?;
                    let (params, wet, on) = cfx_mapping(info, cfx.amount);
                    return Some(Desired {
                        key: cfx.kind.clone(),
                        params,
                        wet,
                        division: 1.0,
                        on: Some(on),
                    });
                }
                None
            }
        }
    }

    // ----- engine plumbing -----

    fn send(env: &mut FxEnv, command: EngineCommand) {
        if let Some(h) = env.handle.as_deref_mut()
            && h.send(command).is_err()
        {
            tracing::warn!(?command, "engine command ring full, FX command dropped");
        }
    }

    fn push_full(&mut self, env: &mut FxEnv, a: Addr, d: &Desired) {
        let Some(slot) = a.id() else { return };
        Self::send(env, EngineCommand::SetFxMix { slot, wet: d.wet });
        if d.division.is_finite() && d.division > 0.0 {
            Self::send(
                env,
                EngineCommand::SetFxBeatDivision {
                    slot,
                    beats: Beats(d.division),
                },
            );
        }
        for (i, v) in d.params.iter().enumerate() {
            Self::send(
                env,
                EngineCommand::SetFxParam {
                    slot,
                    param: ParamId(i as u32),
                    value: *v,
                },
            );
        }
        if let Some(on) = d.on {
            Self::send(env, EngineCommand::SetFxEnabled { slot, on });
        }
    }

    fn has_pending(&self, a: Addr) -> bool {
        self.pending.iter().any(|p| p.addr == a)
    }

    /// Queues an effect swap on `a`: disable now, install after [`SWAP_WAIT`]
    /// (at once when the slot is empty).
    fn begin_swap(&mut self, env: &mut FxEnv, a: Addr, then: Then) {
        if let Some(p) = self.pending.iter_mut().find(|p| p.addr == a) {
            if then != Then::Nothing {
                p.then = then;
            }
            return;
        }
        let ready = if self.installed(a).is_some() {
            if let Some(slot) = a.id() {
                Self::send(env, EngineCommand::SetFxEnabled { slot, on: false });
            }
            env.now + SWAP_WAIT
        } else {
            env.now
        };
        self.pending.push(Pending {
            addr: a,
            ready,
            then,
        });
    }

    /// Brings one engine slot in line with the model.
    fn reconcile(&mut self, env: &mut FxEnv, a: Addr) {
        if self.has_pending(a) {
            return;
        }
        match self.desired(a) {
            None => {
                if self.installed(a).is_some()
                    && let Some(slot) = a.id()
                {
                    Self::send(env, EngineCommand::SetFxEnabled { slot, on: false });
                }
            }
            Some(d) => {
                if self.installed(a).as_ref() == Some(&d.key) {
                    self.push_full(env, a, &d);
                } else {
                    self.begin_swap(env, a, Then::Nothing);
                }
            }
        }
    }

    fn reconcile_deck(&mut self, env: &mut FxEnv, deck: DeckId) {
        for s in 0..FX_SLOTS_PER_UNIT as u8 {
            self.reconcile(env, Addr::Deck(deck, s));
        }
    }

    fn reconcile_all(&mut self, env: &mut FxEnv) {
        self.refresh_blocked();
        for d in 0..MVP_DECKS {
            if let Some(deck) = DeckId::new(d as u8) {
                self.reconcile_deck(env, deck);
            }
        }
        for b in 0..FX_SEND_BUSES as u8 {
            for s in 0..FX_SLOTS_PER_UNIT as u8 {
                self.reconcile(env, Addr::Send(b, s));
            }
        }
    }

    /// After an engine (re)start: nothing is installed, replay everything.
    pub(crate) fn resync(&mut self, env: &mut FxEnv) {
        self.installed_deck = Default::default();
        self.installed_send = Default::default();
        self.pending.clear();
        self.reconcile_all(env);
        for u in 0..BEAT_FX_UNITS {
            self.push_tempo(env, u);
        }
        for d in 0..MVP_DECKS {
            for b in 0..FX_SEND_BUSES {
                self.push_send_level(env, d, b);
            }
        }
        for b in 0..FX_SEND_BUSES {
            self.push_send_tap(env, b);
        }
        self.run_pending(env);
    }

    /// Installs swapped-in effects whose wait is over.
    fn run_pending(&mut self, env: &mut FxEnv) {
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].ready > env.now {
                i += 1;
                continue;
            }
            let (addr, then) = (self.pending[i].addr, self.pending[i].then);
            let Some(d) = self.desired(addr) else {
                self.pending.remove(i);
                continue;
            };
            if self.installed(addr).as_ref() == Some(&d.key) {
                self.pending.remove(i);
                self.push_full(env, addr, &d);
                continue;
            }
            let (Some(slot), true) = (addr.id(), env.handle.is_some()) else {
                self.pending.remove(i);
                continue;
            };
            let mut fx = match self.registry.create(&d.key) {
                Ok(fx) => fx,
                Err(err) => {
                    tracing::warn!(%err, "could not create effect");
                    self.pending.remove(i);
                    continue;
                }
            };
            for (n, v) in d.params.iter().enumerate() {
                fx.set_parameter(ParamId(n as u32), *v);
            }
            let handle = env.handle.as_deref_mut().expect("checked above");
            match handle.install_effect(slot, fx) {
                Err(_) => {
                    // Hand-off ring full: try again next tick.
                    i += 1;
                }
                Ok(()) => {
                    *self.installed(addr) = Some(d.key.clone());
                    self.pending.remove(i);
                    self.push_full(env, addr, &d);
                    self.finish(env, addr, then);
                }
            }
        }
    }

    fn now_frames(env: &FxEnv) -> Frames {
        env.tel.map_or(Frames(0), |t| t.engine_time)
    }

    fn finish(&mut self, env: &mut FxEnv, addr: Addr, then: Then) {
        let Some(slot) = addr.id() else { return };
        match then {
            Then::Nothing => {}
            Then::Engage(at) => {
                if let Some(h) = env.handle.as_deref_mut() {
                    h.schedule_fx_enabled(slot, true, at, true);
                }
            }
            Then::Release { from, hold } => self.do_release(env, from, slot, hold),
        }
    }

    fn do_release(&mut self, env: &mut FxEnv, from: Option<Addr>, release: FxSlotId, hold: Frames) {
        let at = Self::now_frames(env);
        let Some(h) = env.handle.as_deref_mut() else {
            return;
        };
        match from.and_then(Addr::id) {
            Some(from) => {
                h.release_fx(from, release, at, hold);
            }
            None => {
                h.schedule_fx_enabled(release, true, at, true);
                h.schedule_fx_enabled(release, false, Frames(at.0.saturating_add(hold.0)), false);
            }
        }
    }

    // ----- tick -----

    /// Periodic work: pending installs, override expiry, persistence, leader
    /// follow-up for mirrored units.
    pub(crate) fn tick(&mut self, env: &mut FxEnv) {
        self.run_pending(env);
        for d in 0..MVP_DECKS {
            let expired = self.overrides[d]
                .as_ref()
                .is_some_and(|o| o.expires.is_some_and(|t| t <= env.now));
            if expired {
                self.overrides[d] = None;
                if let Some(deck) = DeckId::new(d as u8) {
                    self.reconcile(env, Addr::Deck(deck, UTIL));
                }
            }
        }
        if env.leader != self.last_leader {
            self.last_leader = env.leader;
            for u in 0..BEAT_FX_UNITS {
                if self.state.units[u].assign == FxAssign::Both {
                    self.push_tempo(env, u);
                }
            }
        }
        if self.persist_at.is_some_and(|t| t <= env.now) {
            self.flush(env.library);
        }
    }

    /// Writes the state now if a write is waiting.
    pub(crate) fn flush(&mut self, library: Option<&Library>) {
        if self.persist_at.take().is_none() {
            return;
        }
        let Some(lib) = library else { return };
        match serde_json::to_string(&self.state) {
            Ok(json) => {
                if let Err(err) = lib.set_setting(STATE_KEY, &json) {
                    tracing::warn!(%err, "could not store the FX state");
                }
            }
            Err(err) => tracing::warn!(%err, "could not serialise the FX state"),
        }
    }

    fn changed(&mut self, env: &FxEnv) {
        self.dirty = true;
        self.persist_at = Some(env.now + PERSIST_DELAY);
    }

    // ----- tempo, sends -----

    fn push_tempo(&mut self, env: &mut FxEnv, unit: usize) {
        let u = &self.state.units[unit];
        let both = u.assign == FxAssign::Both;
        let source = match (u.bpm_source, u.tap_bpm) {
            (FxBpmSource::Tap, Some(bpm)) => Some(FxTempoSource::Tap(Bpm(bpm))),
            _ => None,
        };
        let decks = self.unit_decks(unit);
        let first = decks.first().copied();
        let Some(h) = env.handle.as_deref_mut() else {
            return;
        };
        for deck in decks {
            let source = source.unwrap_or(if both {
                FxTempoSource::Deck(env.leader.or(first).unwrap_or(deck))
            } else {
                FxTempoSource::Auto
            });
            h.set_fx_tempo_source(FxUnit::Deck(deck), source);
        }
    }

    fn push_send_level(&mut self, env: &mut FxEnv, deck: usize, bus: usize) {
        if let Some(d) = DeckId::new(deck as u8) {
            Self::send(
                env,
                EngineCommand::SetFxSend {
                    deck: d,
                    bus: bus as u8,
                    level: Gain(self.state.send_levels[deck][bus]),
                },
            );
        }
    }

    fn push_send_tap(&mut self, env: &mut FxEnv, bus: usize) {
        let tap = if self.state.sends[bus].post_fader {
            SendTap::PostFader
        } else {
            SendTap::PreFader
        };
        if let Some(h) = env.handle.as_deref_mut() {
            h.set_fx_send_tap(bus as u8, tap);
        }
    }

    // ----- operations -----

    fn slot_mut(&mut self, r: FxSlotRef) -> Result<&mut FxSlotDto, CoreError> {
        let slot = usize::from(r.slot);
        if slot >= FX_SLOTS_PER_UNIT {
            return Err(invalid("effect slot out of range"));
        }
        let unit = usize::from(r.unit.unit);
        match r.unit.target {
            FxTarget::Beat => self.state.units.get_mut(unit).map(|u| &mut u.slots[slot]),
            FxTarget::Send => self.state.sends.get_mut(unit).map(|u| &mut u.slots[slot]),
        }
        .ok_or_else(|| invalid("effect unit out of range"))
    }

    fn slot_addrs(&self, r: FxSlotRef) -> Vec<Addr> {
        match r.unit.target {
            FxTarget::Beat => self
                .unit_decks(usize::from(r.unit.unit))
                .into_iter()
                .map(|d| Addr::Deck(d, r.slot))
                .collect(),
            FxTarget::Send => vec![Addr::Send(r.unit.unit, r.slot)],
        }
    }

    fn check_unit(&self, u: FxUnitRef) -> Result<(), CoreError> {
        let limit = match u.target {
            FxTarget::Beat => BEAT_FX_UNITS,
            FxTarget::Send => FX_SEND_BUSES,
        };
        if usize::from(u.unit) < limit {
            Ok(())
        } else {
            Err(invalid("effect unit out of range"))
        }
    }

    fn beat_unit(&self, unit: u8) -> Result<usize, CoreError> {
        let u = usize::from(unit);
        if u < BEAT_FX_UNITS {
            Ok(u)
        } else {
            Err(invalid("Beat FX unit out of range"))
        }
    }

    fn known(&self, key: &EffectKey) -> Result<&FxEffectDto, CoreError> {
        self.effects
            .get(key)
            .ok_or_else(|| invalid(format!("unknown effect {}:{}", key.provider, key.id)))
    }

    /// Decks a unit reference touches.
    fn decks_for(&self, u: FxUnitRef) -> Vec<DeckId> {
        match u.target {
            FxTarget::Beat => self.unit_decks(usize::from(u.unit)),
            FxTarget::Send => Vec::new(),
        }
    }

    /// Re-syncs the engine slots behind a unit.
    fn refresh_unit(&mut self, env: &mut FxEnv, u: FxUnitRef) {
        self.refresh_blocked();
        match u.target {
            FxTarget::Beat => {
                for d in self.decks_for(u) {
                    self.reconcile_deck(env, d);
                }
            }
            FxTarget::Send => {
                for s in 0..FX_SLOTS_PER_UNIT as u8 {
                    self.reconcile(env, Addr::Send(u.unit, s));
                }
            }
        }
    }

    /// Applies one change to the model and the engine.
    pub(crate) fn apply(&mut self, env: &mut FxEnv, op: FxOp) -> Result<(), CoreError> {
        match op {
            FxOp::SetAssign { unit, assign } => {
                let u = self.beat_unit(unit)?;
                let want = decks_of(assign);
                self.state.units[u].assign = assign;
                // A channel has one chain: the other unit loses what this one takes.
                for (v, other) in self.state.units.iter_mut().enumerate() {
                    if v != u {
                        let mut decks = decks_of(other.assign);
                        for (d, w) in decks.iter_mut().zip(want) {
                            *d &= !w;
                        }
                        other.assign = assign_of(decks);
                    }
                }
                self.reconcile_all(env);
                for v in 0..BEAT_FX_UNITS {
                    self.push_tempo(env, v);
                }
            }
            FxOp::SetMode { unit, mode } => {
                let u = self.beat_unit(unit)?;
                self.state.units[u].mode = mode;
                self.refresh_unit(env, unit_ref(FxTarget::Beat, unit));
            }
            FxOp::SetEffect { slot, effect } => {
                self.check_unit(slot.unit)?;
                let send = slot.unit.target == FxTarget::Send;
                let new = match &effect {
                    Some(key) => {
                        self.known(key)?;
                        self.default_slot(key, send)
                    }
                    None => FxSlotDto::empty(true),
                };
                let s = self.slot_mut(slot)?;
                let enabled = s.enabled;
                *s = new;
                s.enabled = enabled;
                self.refresh_unit(env, slot.unit);
            }
            FxOp::SetParam { slot, param, value } => {
                finite(value)?;
                let value = value.clamp(0.0, 1.0);
                let s = self.slot_mut(slot)?;
                let i = param.0 as usize;
                if i >= s.params.len() {
                    return Err(invalid("parameter out of range"));
                }
                s.params[i] = value;
                self.push_slot_param(env, slot, i, value);
            }
            FxOp::SetLevel { slot, value } => {
                finite(value)?;
                let key = self
                    .slot_mut(slot)?
                    .effect
                    .clone()
                    .ok_or_else(|| invalid("the slot is empty"))?;
                let primary = self
                    .known(&key)?
                    .primary
                    .ok_or_else(|| invalid("the effect has no primary parameter"))?;
                return self.apply(
                    env,
                    FxOp::SetParam {
                        slot,
                        param: primary,
                        value,
                    },
                );
            }
            FxOp::SetWet { slot, wet } => {
                finite(wet)?;
                let wet = wet.clamp(0.0, 1.0);
                self.slot_mut(slot)?.wet = wet;
                self.push_slot_with(env, slot, |a, slot_id| {
                    let _ = a;
                    EngineCommand::SetFxMix { slot: slot_id, wet }
                });
            }
            FxOp::SetDivision { slot, beats } => {
                if !(beats.is_finite() && beats > 0.0) {
                    return Err(invalid("beat division must be above zero"));
                }
                self.slot_mut(slot)?.division = beats;
                self.push_slot_with(env, slot, |_, slot_id| EngineCommand::SetFxBeatDivision {
                    slot: slot_id,
                    beats: Beats(beats),
                });
            }
            FxOp::StepDivision { unit, direction } => {
                self.check_unit(unit)?;
                for s in 0..FX_SLOTS_PER_UNIT as u8 {
                    let r = FxSlotRef { unit, slot: s };
                    let Some(key) = self.slot_mut(r)?.effect.clone() else {
                        continue;
                    };
                    let list = divisions_of(&key);
                    if list.is_empty() {
                        continue;
                    }
                    let cur = self.slot_mut(r)?.division;
                    let at = nearest(&list, cur);
                    let next = if direction < 0 {
                        at.saturating_sub(1)
                    } else {
                        (at + 1).min(list.len() - 1)
                    };
                    self.apply(
                        env,
                        FxOp::SetDivision {
                            slot: r,
                            beats: list[next],
                        },
                    )?;
                }
            }
            FxOp::SetSlotEnabled { slot, on } => {
                self.slot_mut(slot)?.enabled = on;
                self.refresh_unit(env, slot.unit);
            }
            FxOp::SetUnitOn { unit, on } => {
                let u = self.beat_unit(unit)?;
                self.state.units[u].on = on;
                self.refresh_unit(env, unit_ref(FxTarget::Beat, unit));
            }
            FxOp::Release { unit } => {
                let u = self.beat_unit(unit)?;
                self.beat_release(env, u);
            }
            FxOp::SetBpmSource { unit, source } => {
                let u = self.beat_unit(unit)?;
                self.state.units[u].bpm_source = source;
                self.push_tempo(env, u);
            }
            FxOp::Tap { unit } => {
                let u = self.beat_unit(unit)?;
                let t = env.now.duration_since(self.epoch).as_secs_f64();
                push_tap(&mut self.taps[u], t);
                if let Some(bpm) = tap_bpm(&self.taps[u]) {
                    self.state.units[u].tap_bpm = Some(bpm);
                    self.state.units[u].bpm_source = FxBpmSource::Tap;
                    self.push_tempo(env, u);
                }
            }
            FxOp::SetReleaseEffect {
                unit,
                effect,
                hold_ms,
            } => {
                let u = self.beat_unit(unit)?;
                if let Some(key) = &effect {
                    self.known(key)?;
                }
                let unit = &mut self.state.units[u];
                unit.release_effect = effect;
                unit.release_hold_ms = hold_ms.clamp(50, 10_000);
            }
            FxOp::SetCfxKind { deck, effect } => {
                if !self.state.cfx_kinds.contains(&effect) {
                    return Err(invalid("not a Sound Color FX type"));
                }
                self.state.cfx[deck.index()].kind = effect;
                if deck.index() < MVP_DECKS {
                    self.reconcile(env, Addr::Deck(deck, UTIL));
                }
            }
            FxOp::SetCfx { deck, amount } => {
                finite(amount)?;
                if deck.index() >= MVP_DECKS {
                    return Err(invalid("deck out of range"));
                }
                self.state.cfx[deck.index()].amount = amount.clamp(-1.0, 1.0);
                self.push_cfx(env, deck);
            }
            FxOp::SetSendLevel { deck, bus, level } => {
                finite(level)?;
                if deck.index() >= MVP_DECKS || usize::from(bus) >= FX_SEND_BUSES {
                    return Err(invalid("send out of range"));
                }
                self.state.send_levels[deck.index()][usize::from(bus)] = level.clamp(0.0, 1.0);
                self.push_send_level(env, deck.index(), usize::from(bus));
            }
            FxOp::SetSendTap { bus, post_fader } => {
                let b = usize::from(bus);
                if b >= FX_SEND_BUSES {
                    return Err(invalid("send bus out of range"));
                }
                self.state.sends[b].post_fader = post_fader;
                self.push_send_tap(env, b);
            }
            FxOp::ApplyPreset { slot, preset } => {
                self.check_unit(slot.unit)?;
                self.known(&preset.effect)?;
                self.apply_preset(env, slot, &preset)?;
            }
            FxOp::ApplyChain { unit, name } => {
                self.check_unit(unit)?;
                let chain = self
                    .chains
                    .iter()
                    .find(|c| c.name == name)
                    .cloned()
                    .ok_or_else(|| invalid("no chain with that name"))?;
                self.apply_chain(env, unit, &chain)?;
            }
            FxOp::SetPad { index, pad } => {
                if usize::from(index) >= PAD_FX_COUNT {
                    return Err(invalid("pad index out of range"));
                }
                if let Some(key) = &pad.effect {
                    self.known(key)?;
                }
                if !(pad.division.is_finite() && pad.division > 0.0) {
                    return Err(invalid("beat division must be above zero"));
                }
                self.state.pads[usize::from(index)] = pad;
            }
        }
        self.changed(env);
        self.run_pending(env);
        Ok(())
    }

    fn push_slot_with(
        &mut self,
        env: &mut FxEnv,
        slot: FxSlotRef,
        make: impl Fn(Addr, FxSlotId) -> EngineCommand,
    ) {
        for a in self.slot_addrs(slot) {
            let wanted = self.desired(a).map(|d| d.key);
            if wanted.is_some() && self.installed(a).as_ref() == wanted.as_ref() {
                if let Some(id) = a.id() {
                    Self::send(env, make(a, id));
                }
            } else {
                self.reconcile(env, a);
            }
        }
    }

    fn push_slot_param(&mut self, env: &mut FxEnv, slot: FxSlotRef, index: usize, value: f32) {
        self.push_slot_with(env, slot, |_, id| EngineCommand::SetFxParam {
            slot: id,
            param: ParamId(index as u32),
            value,
        });
    }

    fn push_cfx(&mut self, env: &mut FxEnv, deck: DeckId) {
        let a = Addr::Deck(deck, UTIL);
        let usable =
            self.overrides[deck.index()].is_none() && !self.state.utility_blocked[deck.index()];
        let Some(d) = self.desired(a).filter(|_| usable) else {
            return;
        };
        if self.has_pending(a) || self.installed(a).as_ref() != Some(&d.key) {
            self.reconcile(env, a);
            return;
        }
        let Some(slot) = a.id() else { return };
        let info = self.effects.get(&d.key).and_then(|i| i.primary);
        // Only the parameter the knob moves, the mix and the on-state change.
        let moved = if d.key.id == "colour_filter" {
            Some(ParamId(0))
        } else {
            info
        };
        if let Some(p) = moved
            && let Some(v) = d.params.get(p.0 as usize)
        {
            Self::send(
                env,
                EngineCommand::SetFxParam {
                    slot,
                    param: p,
                    value: *v,
                },
            );
        }
        Self::send(env, EngineCommand::SetFxMix { slot, wet: d.wet });
        if let Some(on) = d.on {
            Self::send(env, EngineCommand::SetFxEnabled { slot, on });
        }
    }

    fn apply_preset(
        &mut self,
        env: &mut FxEnv,
        slot: FxSlotRef,
        preset: &FxPreset,
    ) -> Result<(), CoreError> {
        let send = slot.unit.target == FxTarget::Send;
        let mut new = self.default_slot(&preset.effect, send);
        for (id, v) in &preset.params {
            if let Some(p) = new.params.get_mut(id.0 as usize)
                && v.is_finite()
            {
                *p = v.clamp(0.0, 1.0);
            }
        }
        new.wet = preset.wet.clamp(0.0, 1.0);
        if preset.beat_division.0.is_finite() && preset.beat_division.0 > 0.0 {
            new.division = preset.beat_division.0;
        }
        let s = self.slot_mut(slot)?;
        new.enabled = s.enabled;
        *s = new;
        self.refresh_unit(env, slot.unit);
        Ok(())
    }

    fn apply_chain(
        &mut self,
        env: &mut FxEnv,
        unit: FxUnitRef,
        chain: &FxChainPresetDto,
    ) -> Result<(), CoreError> {
        if chain.slots.len() > FX_SLOTS_PER_UNIT {
            return Err(invalid("the chain has too many effects"));
        }
        for (i, cs) in chain.slots.iter().enumerate() {
            self.known(&cs.effect)?;
            let preset = FxPreset {
                effect: cs.effect.clone(),
                name: String::new(),
                params: cs.params.clone(),
                wet: cs.wet,
                beat_division: Beats(cs.division),
            };
            let r = FxSlotRef {
                unit,
                slot: i as u8,
            };
            self.apply_preset(env, r, &preset)?;
            self.slot_mut(r)?.enabled = cs.enabled;
        }
        for i in chain.slots.len()..FX_SLOTS_PER_UNIT {
            *self.slot_mut(FxSlotRef {
                unit,
                slot: i as u8,
            })? = FxSlotDto::empty(true);
        }
        if unit.target == FxTarget::Beat && chain.slots.len() > 1 {
            self.state.units[usize::from(unit.unit)].mode = FxMode::Multi;
        }
        self.refresh_unit(env, unit);
        Ok(())
    }

    // ----- release and pad FX -----

    /// Frame of the next beat of `deck` when quantize is on and a grid plays.
    fn quantized_frame(&self, env: &FxEnv, deck: DeckId) -> Frames {
        let now = Self::now_frames(env);
        let Some(t) = env.tel else { return now };
        let d = &t.decks[deck.index()];
        match d.bpm {
            Some(bpm)
                if env.quantize[deck.index()]
                    && d.playing
                    && bpm.0 > 0.0
                    && d.beat_phase > 0.001 =>
            {
                let secs = f64::from(1.0 - d.beat_phase) * 60.0 / bpm.0;
                Frames(now.0 + (secs * f64::from(t.sample_rate.0)) as u64)
            }
            _ => now,
        }
    }

    fn default_override(
        &self,
        key: &EffectKey,
        division: f64,
        held_by: Option<u8>,
    ) -> Option<Override> {
        let info = self.effects.get(key)?;
        let division = match info.divisions.as_slice() {
            [] => division,
            list => list[nearest(list, division)],
        };
        Some(Override {
            key: key.clone(),
            params: info.params.iter().map(|p| p.default_normalized()).collect(),
            wet: factory_presets(&key.id).first().map_or(1.0, |p| p.wet),
            division,
            held_by,
            expires: None,
        })
    }

    /// SHIFT+ON/OFF of a Beat FX unit.
    fn beat_release(&mut self, env: &mut FxEnv, unit: usize) {
        let (effect, hold_ms) = {
            let u = &self.state.units[unit];
            (u.release_effect.clone(), u.release_hold_ms)
        };
        let division = self.state.units[unit].slots[0].division;
        let from_slot = (0..UTIL).rev().find(|s| self.slot_in_use(unit, *s));
        self.state.units[unit].on = false;
        for deck in self.unit_decks(unit) {
            self.release_on_deck(env, deck, effect.as_ref(), division, hold_ms, from_slot);
        }
        self.refresh_blocked();
    }

    fn release_on_deck(
        &mut self,
        env: &mut FxEnv,
        deck: DeckId,
        effect: Option<&EffectKey>,
        division: f64,
        hold_ms: u32,
        from_slot: Option<u8>,
    ) {
        let d = deck.index();
        let util = Addr::Deck(deck, UTIL);
        let at = Self::now_frames(env);
        let rate = env.tel.map_or(48_000, |t| t.sample_rate.0);
        let hold = Frames(u64::from(hold_ms) * u64::from(rate) / 1000);
        // The other Beat FX slots of the deck switch off with the unit.
        let from = from_slot.map(|s| Addr::Deck(deck, s));
        for s in 0..UTIL {
            if Some(s) != from_slot
                && let Some(id) = Addr::Deck(deck, s).id()
                && let Some(h) = env.handle.as_deref_mut()
            {
                h.schedule_fx_enabled(id, false, at, false);
            }
        }
        let blocked = self.state.utility_blocked[d];
        let Some(key) = effect.filter(|_| !blocked) else {
            // No release effect or no free slot: plain off.
            if let Some(id) = from.and_then(Addr::id)
                && let Some(h) = env.handle.as_deref_mut()
            {
                h.schedule_fx_enabled(id, false, at, false);
            }
            return;
        };
        let Some(mut o) = self.default_override(key, division, None) else {
            return;
        };
        let tail = self.effects.get(key).is_some_and(|i| i.has_tail);
        o.expires = Some(
            env.now
                + Duration::from_millis(u64::from(hold_ms))
                + if tail { PAD_TAIL_LONG } else { PAD_TAIL },
        );
        self.overrides[d] = Some(o);
        let then = Then::Release { from, hold };
        if !self.has_pending(util) && self.installed(util).as_ref() == Some(key) {
            let desired = self.desired(util);
            if let Some(desired) = desired {
                self.push_full(env, util, &desired);
            }
            if let Some(slot) = util.id() {
                self.do_release(env, from, slot, hold);
            }
        } else {
            self.begin_swap(env, util, then);
        }
    }

    /// Pad FX edge. `index` is `bank * 16 + shift * 8 + pad`.
    pub(crate) fn pad(
        &mut self,
        env: &mut FxEnv,
        deck: DeckId,
        index: usize,
        pressed: bool,
    ) -> Result<(), CoreError> {
        if index >= PAD_FX_COUNT || deck.index() >= MVP_DECKS {
            return Err(invalid("pad or deck out of range"));
        }
        let d = deck.index();
        let util = Addr::Deck(deck, UTIL);
        let tag = index as u8;
        if !pressed {
            let held = self.overrides[d]
                .as_ref()
                .is_some_and(|o| o.held_by == Some(tag));
            if held {
                let now = Self::now_frames(env);
                if let Some(id) = util.id()
                    && let Some(h) = env.handle.as_deref_mut()
                {
                    h.schedule_fx_enabled(id, false, now, true);
                }
                let tail = self.overrides[d]
                    .as_ref()
                    .and_then(|o| self.effects.get(&o.key))
                    .is_some_and(|i| i.has_tail);
                let waiting = self
                    .pending
                    .iter()
                    .any(|p| p.addr == util && matches!(p.then, Then::Engage(_)));
                if waiting {
                    self.pending.retain(|p| p.addr != util);
                }
                if let Some(o) = self.overrides[d].as_mut() {
                    o.expires = Some(env.now + if tail { PAD_TAIL_LONG } else { PAD_TAIL });
                }
            }
            return Ok(());
        }
        let assign = self.state.pads[index].clone();
        let Some(key) = assign.effect.clone() else {
            return Ok(());
        };
        if self.state.utility_blocked[d] {
            return Ok(());
        }
        if assign.release {
            let (hold_ms, from_slot) = match self.owner(d) {
                Some(u) => (
                    self.state.units[u].release_hold_ms,
                    (0..UTIL).rev().find(|s| self.slot_in_use(u, *s)),
                ),
                None => (2000, None),
            };
            if let Some(u) = self.owner(d) {
                self.state.units[u].on = false;
                self.dirty = true;
            }
            self.release_on_deck(env, deck, Some(&key), assign.division, hold_ms, from_slot);
            return Ok(());
        }
        let Some(o) = self.default_override(&key, assign.division, Some(tag)) else {
            return Err(invalid("pad effect is unknown"));
        };
        let at = self.quantized_frame(env, deck);
        self.overrides[d] = Some(o);
        if !self.has_pending(util) && self.installed(util).as_ref() == Some(&key) {
            if let Some(desired) = self.desired(util) {
                self.push_full(env, util, &desired);
            }
            if let Some(id) = util.id()
                && let Some(h) = env.handle.as_deref_mut()
            {
                h.schedule_fx_enabled(id, true, at, true);
            }
        } else {
            self.begin_swap(env, util, Then::Engage(at));
        }
        self.run_pending(env);
        Ok(())
    }

    // ----- presets -----

    /// Slot as a preset named `name`.
    pub(crate) fn slot_preset(
        &mut self,
        slot: FxSlotRef,
        name: &str,
    ) -> Result<FxPreset, CoreError> {
        self.check_unit(slot.unit)?;
        let s = self.slot_mut(slot)?.clone();
        let effect = s.effect.ok_or_else(|| invalid("the slot is empty"))?;
        Ok(FxPreset {
            effect,
            name: name.trim().to_owned(),
            params: s
                .params
                .iter()
                .enumerate()
                .map(|(i, v)| (ParamId(i as u32), *v))
                .collect(),
            wet: s.wet,
            beat_division: Beats(s.division),
        })
    }

    /// Stores a user preset (replacing one with the same effect and name).
    pub(crate) fn save_preset(
        &mut self,
        lib: Option<&Library>,
        preset: FxPreset,
    ) -> Result<(), CoreError> {
        if preset.name.trim().is_empty() {
            return Err(invalid("the preset needs a name"));
        }
        if let Some(lib) = lib {
            lib.save_fx_preset(&to_stored(&preset))
                .map_err(|e| invalid(e.to_string()))?;
        }
        self.user_presets
            .retain(|p| !(p.effect == preset.effect && p.name == preset.name));
        self.user_presets.push(preset);
        self.user_presets
            .sort_by(|a, b| (&a.effect, &a.name).cmp(&(&b.effect, &b.name)));
        Ok(())
    }

    /// Deletes a user preset.
    pub(crate) fn delete_preset(
        &mut self,
        lib: Option<&Library>,
        effect: &EffectKey,
        name: &str,
    ) -> Result<(), CoreError> {
        if let Some(lib) = lib {
            lib.delete_fx_preset(&effect.provider, &effect.id, name)
                .map_err(|e| invalid(e.to_string()))?;
        }
        self.user_presets
            .retain(|p| !(p.effect == *effect && p.name == name));
        Ok(())
    }

    /// Stores the effects of a unit as a chain preset.
    pub(crate) fn save_chain(
        &mut self,
        lib: Option<&Library>,
        unit: FxUnitRef,
        name: &str,
    ) -> Result<(), CoreError> {
        self.check_unit(unit)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(invalid("the chain needs a name"));
        }
        let mut slots = Vec::new();
        for s in 0..FX_SLOTS_PER_UNIT as u8 {
            let r = FxSlotRef { unit, slot: s };
            let slot = self.slot_mut(r)?.clone();
            if let Some(effect) = slot.effect {
                slots.push(FxChainSlotDto {
                    effect,
                    params: slot
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, v)| (ParamId(i as u32), *v))
                        .collect(),
                    wet: slot.wet,
                    division: slot.division,
                    enabled: slot.enabled,
                });
            }
        }
        if slots.is_empty() {
            return Err(invalid("the unit has no effects to store"));
        }
        self.chains.retain(|c| c.name != name);
        self.chains.push(FxChainPresetDto {
            name: name.to_owned(),
            slots,
        });
        self.chains.sort_by(|a, b| a.name.cmp(&b.name));
        self.store_chains(lib)
    }

    /// Deletes a chain preset.
    pub(crate) fn delete_chain(
        &mut self,
        lib: Option<&Library>,
        name: &str,
    ) -> Result<(), CoreError> {
        self.chains.retain(|c| c.name != name);
        self.store_chains(lib)
    }

    fn store_chains(&self, lib: Option<&Library>) -> Result<(), CoreError> {
        let Some(lib) = lib else { return Ok(()) };
        let json = serde_json::to_string(&self.chains).map_err(|e| invalid(e.to_string()))?;
        lib.set_setting(CHAINS_KEY, &json)
            .map_err(|e| invalid(e.to_string()))
    }
}

fn unit_ref(target: FxTarget, unit: u8) -> FxUnitRef {
    FxUnitRef { target, unit }
}

fn finite(v: f32) -> Result<(), CoreError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(invalid("value is not a finite number"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_tempo_averages_the_intervals() {
        // 0.5 s apart is 120 BPM.
        let taps = [10.0, 10.5, 11.0, 11.5];
        assert!((tap_bpm(&taps).unwrap() - 120.0).abs() < 1e-9);
        assert_eq!(tap_bpm(&[1.0]), None);
        assert_eq!(tap_bpm(&[]), None);
        // Clamped to the supported range.
        assert_eq!(tap_bpm(&[0.0, 5.0]).unwrap(), MIN_BPM);
        assert_eq!(tap_bpm(&[0.0, 0.16]).unwrap(), MAX_TAP_BPM);
    }

    #[test]
    fn taps_restart_after_a_gap_and_ignore_bounces() {
        let mut taps = Vec::new();
        for t in [0.0, 0.5, 0.51, 1.0] {
            push_tap(&mut taps, t);
        }
        assert_eq!(taps, vec![0.0, 0.5, 1.0]);
        push_tap(&mut taps, 5.0);
        assert_eq!(taps, vec![5.0]);
        for i in 0..20 {
            push_tap(&mut taps, 5.5 + f64::from(i) * 0.5);
        }
        assert_eq!(taps.len(), TAP_WINDOW);
    }

    #[test]
    fn division_lists_follow_the_catalog_range() {
        let echo = divisions_of(&builtin("echo"));
        assert!(echo.windows(2).all(|w| w[0] < w[1]));
        let spec = catalog::find("echo").unwrap().sync.unwrap();
        assert!(
            echo.iter()
                .all(|d| *d >= spec.min_beats && *d <= spec.max_beats)
        );
        assert!(divisions_of(&builtin("noise")).is_empty());
        assert!(divisions_of(&builtin("colour_filter")).is_empty());
        assert_eq!(nearest(&[0.25, 0.5, 1.0], 0.6), 1);
    }

    #[test]
    fn default_pads_use_known_effects_in_range() {
        for (i, (id, division, release)) in DEFAULT_PADS.iter().enumerate() {
            let spec = catalog::find(id).unwrap_or_else(|| panic!("pad {i}: unknown {id}"));
            if let Some(sync) = spec.sync {
                assert!(
                    *division >= sync.min_beats && *division <= sync.max_beats,
                    "pad {i}: {id} division {division}"
                );
            }
            assert_eq!(*release, i >= 26, "pad {i}");
        }
    }

    #[test]
    fn cfx_centre_is_neutral() {
        let c = FxController::new(None);
        let info = c.effects.get(&builtin("colour_filter")).unwrap();
        let (params, wet, on) = cfx_mapping(info, 0.0);
        assert!(!on);
        assert_eq!(wet, 1.0);
        assert_eq!(params[0], 0.5);
        let (params, _, on) = cfx_mapping(info, -1.0);
        assert!(on);
        assert_eq!(params[0], 0.0);
        let noise = c.effects.get(&builtin("noise")).unwrap();
        let (_, wet, on) = cfx_mapping(noise, 0.5);
        assert!(on);
        assert_eq!(wet, 0.5);
    }

    #[test]
    fn default_state_is_consistent() {
        let c = FxController::new(None);
        let s = c.state();
        assert_eq!(s.units.len(), BEAT_FX_UNITS);
        assert_eq!(s.pads.len(), PAD_FX_COUNT);
        assert_eq!(s.units[0].assign, FxAssign::Ch1);
        assert_eq!(s.units[1].assign, FxAssign::Ch2);
        let echo = &s.units[0].slots[0];
        assert_eq!(echo.params.len(), c.effects[&builtin("echo")].params.len());
        assert!(c.effects[&builtin("vinyl_brake")].release);
        assert_eq!(c.effects[&builtin("echo")].primary, Some(ParamId(1)));
    }
}
