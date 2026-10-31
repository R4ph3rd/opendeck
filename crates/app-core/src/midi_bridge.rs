//! The MIDI bridge: controller events in, application actions out, and
//! application state in, LED feedback out (milestone 6, ADR 0007).
//!
//! [`MidiBridge`] is synchronous and owns no thread. The MIDI service drives
//! it with the worker's events; the headless replay tests drive it with the
//! midi crate's `Pipeline`. It talks to the control thread only through
//! [`AudioController`] and [`LibraryService`], so everything it does is
//! visible in the same state the UI reads.
//!
//! # Conventions
//!
//! - Toggles read the current state first (`deck_state`, `mixer_state`,
//!   `fx_state`), so a UI change and a controller press never disagree.
//! - Pad modes are per deck and shared through [`PadModes`]. A pad press of a
//!   mode other than the deck's active mode is dropped.
//! - Beat FX: the hardware has one section. CH SELECT reports two flags;
//!   section controls apply to the Beat FX unit(s) that own the selected
//!   channel(s) in the FX model. CH SELECT does not move channels between
//!   units.
//! - The tempo fader uses soft takeover (see [`Takeover`]).
//! - Feedback is computed from the newest telemetry, hot cues, FX state and
//!   the slowly refreshed deck and mixer state, and only changes are
//!   returned.

use std::collections::HashMap;

use opendeck_contracts::control::{
    ControlAction, ControlEvent, ControlValue, FeedbackTarget, FeedbackValue,
};
use opendeck_contracts::deck::DeckState;
use opendeck_contracts::ids::{FxUnit, TrackId};
use opendeck_contracts::mixer::MixerState;
use opendeck_contracts::pad::PadMode;
use opendeck_contracts::tempo::SyncMode;
use opendeck_contracts::{
    Beats, DeckId, EngineEvent, EngineTelemetry, Gain, MAX_DECKS, MVP_DECKS, Seconds,
};

use crate::dto::{SortColumnDto, TrackQueryDto};
use crate::fx_types::{
    FxAssign, FxBpmSource, FxOp, FxSlotRef, FxStateDto, FxTarget, FxUnitRef, PADS_PER_LAYER,
};
use crate::midi_types::{BrowseContextDto, BrowseCursorDto, BrowseSourceDto, PadModeChanged};
use crate::pads::{BEAT_LOOP_LENGTHS, PadActions, PadModes, pad_semitones};
use crate::types::CoreEvent;
use crate::{AudioController, LibraryService};

/// Rows the browse cursor moves per encoder tick.
pub const BROWSE_PAGE: u32 = 1;

/// Fast search distance of one SHIFT + jog revolution, in beats.
/// Placeholder until the jog resolution is measured (open question Q02).
pub const SEARCH_BEATS_PER_REVOLUTION: f64 = 32.0;

/// Pitch bend amount of a held bend button (`1.0` is the engine's maximum).
pub const PITCH_BEND_AMOUNT: f32 = 1.0;

/// Taps further apart than this start a new tap tempo measurement.
const TAP_RESET_US: u64 = 2_000_000;

/// Level meter range in dB below full scale mapped onto the LED meter.
/// Placeholder curve (open question Q04).
const METER_RANGE_DB: f32 = 36.0;

/// The tempo fader soft takeover zone, in fader positions (`0..1`).
const TAKEOVER_ZONE: f32 = 0.02;

/// Something the shell should tell the UI about after a control event.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeNotice {
    /// A pad mode button changed a deck's mode.
    PadMode(PadModeChanged),
    /// The browse encoder moved the cursor.
    Browse(BrowseCursorDto),
}

/// Soft takeover for an absolute control whose position the hardware cannot
/// report: the hardware value is ignored until it meets the software value
/// (within a small zone, or by crossing it), then it follows.
#[derive(Debug, Clone, Copy, Default)]
pub struct Takeover {
    engaged: bool,
    last_hw: Option<f32>,
    applied: Option<f32>,
}

impl Takeover {
    /// The software value changed without the control (deck load, range
    /// change): wait for the hardware to meet it again.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feeds a hardware position and the software position (both `0..1`).
    /// True when the hardware value should be applied.
    pub fn feed(&mut self, hardware: f32, software: f32) -> bool {
        if self.engaged
            && self
                .applied
                .is_some_and(|a| (a - software).abs() > TAKEOVER_ZONE)
        {
            // Someone else moved the value (the UI): pick up again.
            self.engaged = false;
        }
        if !self.engaged {
            let near = (hardware - software).abs() <= TAKEOVER_ZONE;
            let crossed = self
                .last_hw
                .is_some_and(|last| (last - software) * (hardware - software) <= 0.0);
            self.engaged = near || crossed;
        }
        self.last_hw = Some(hardware);
        if self.engaged {
            self.applied = Some(hardware);
        }
        self.engaged
    }
}

// ----- curves (mirror ui/src/lib/mixer.ts) -----

const EQ_FLOOR_DB: f32 = -40.0;
const LEVEL_FLOOR_DB: f32 = -48.0;
const LEVEL_UNITY_POSITION: f32 = 0.75;
const LEVEL_MAX_GAIN_DB: f32 = 6.0;

fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Trim knob `0..1` to linear gain; the middle is unity.
pub fn trim_gain(position: f32) -> Gain {
    let p = position.clamp(0.0, 1.0);
    Gain(db_to_gain(
        (p * 2.0 - 1.0) * opendeck_contracts::mixer::TRIM_MAX_GAIN_DB,
    ))
}

/// EQ knob `0..1` to linear gain: `0` kills the band, the middle is unity.
pub fn eq_gain(position: f32) -> Gain {
    let p = position.clamp(0.0, 1.0);
    if p == 0.0 {
        return Gain(0.0);
    }
    if p <= 0.5 {
        return Gain(db_to_gain(EQ_FLOOR_DB * (1.0 - p / 0.5)));
    }
    Gain(db_to_gain(
        ((p - 0.5) / 0.5) * opendeck_contracts::mixer::EQ_MAX_GAIN_DB,
    ))
}

/// Master and headphone level knob `0..1` to linear gain.
pub fn level_gain(position: f32) -> Gain {
    let p = position.clamp(0.0, 1.0);
    if p == 0.0 {
        return Gain(0.0);
    }
    if p <= LEVEL_UNITY_POSITION {
        return Gain(db_to_gain(
            LEVEL_FLOOR_DB * (1.0 - p / LEVEL_UNITY_POSITION),
        ));
    }
    Gain(db_to_gain(
        ((p - LEVEL_UNITY_POSITION) / (1.0 - LEVEL_UNITY_POSITION)) * LEVEL_MAX_GAIN_DB,
    ))
}

/// Peak level (linear) to the `0..1` value of the LED meter. Placeholder
/// curve, open question Q04: `METER_RANGE_DB` below full scale is the bottom.
pub fn meter_level(peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * peak.log10();
    ((db + METER_RANGE_DB) / METER_RANGE_DB).clamp(0.0, 1.0)
}

// ----- browse cursor -----

#[derive(Debug, Clone)]
struct BrowseCursor {
    source: BrowseSourceDto,
    index: Option<u32>,
}

impl Default for BrowseCursor {
    fn default() -> Self {
        Self {
            source: BrowseSourceDto::Collection {
                query: TrackQueryDto {
                    text: None,
                    bpm_min: None,
                    bpm_max: None,
                    keys: Vec::new(),
                    genre: None,
                    min_rating: None,
                    status: None,
                    sort: SortColumnDto::default(),
                    descending: false,
                    offset: 0,
                    limit: 1,
                },
            },
            index: None,
        }
    }
}

impl BrowseCursor {
    /// Number of rows and the track at `index`.
    fn row(&self, library: &LibraryService, index: u32) -> Option<(u64, TrackId)> {
        match &self.source {
            BrowseSourceDto::Collection { query } => {
                let mut q = query.clone();
                q.offset = index;
                q.limit = 1;
                let page = library.query_tracks(q).ok()?;
                Some((page.total, page.rows.first()?.id))
            }
            BrowseSourceDto::Playlist { id } => {
                let entries = library.playlist_tracks(*id).ok()?;
                let entry = entries.get(index as usize)?;
                Some((entries.len() as u64, entry.track.id))
            }
        }
    }

    fn total(&self, library: &LibraryService) -> u64 {
        match &self.source {
            BrowseSourceDto::Collection { query } => {
                let mut q = query.clone();
                q.offset = 0;
                q.limit = 1;
                library.query_tracks(q).map(|p| p.total).unwrap_or(0)
            }
            BrowseSourceDto::Playlist { id } => library
                .playlist_tracks(*id)
                .map(|e| e.len() as u64)
                .unwrap_or(0),
        }
    }

    /// Moves by `rows`, clamped to the list. `None` when the list is empty.
    fn step(&mut self, library: &LibraryService, rows: i64) -> Option<BrowseCursorDto> {
        let total = self.total(library);
        if total == 0 {
            return None;
        }
        let last = i64::try_from(total - 1).unwrap_or(i64::MAX);
        let next = match self.index {
            None => 0,
            Some(i) => (i64::from(i) + rows).clamp(0, last),
        };
        let index = u32::try_from(next).unwrap_or(0);
        let (_, track) = self.row(library, index)?;
        self.index = Some(index);
        Some(BrowseCursorDto { index, track })
    }

    fn selected(&self, library: &LibraryService) -> Option<TrackId> {
        self.row(library, self.index?).map(|(_, t)| t)
    }
}

// ----- the bridge -----

/// The state feedback is derived from.
#[derive(Debug, Clone)]
struct View {
    telemetry: EngineTelemetry,
    have_telemetry: bool,
    decks: [Option<DeckState>; MAX_DECKS],
    mixer: Option<MixerState>,
    hot_cues: [[bool; opendeck_contracts::HOT_CUES_PER_DECK]; MAX_DECKS],
    fx: Option<FxStateDto>,
    /// Main cue point in engine frames, from events and deck state.
    cue_frames: [Option<u64>; MAX_DECKS],
}

impl Default for View {
    fn default() -> Self {
        Self {
            telemetry: EngineTelemetry::default(),
            have_telemetry: false,
            decks: std::array::from_fn(|_| None),
            mixer: None,
            hot_cues: [[false; opendeck_contracts::HOT_CUES_PER_DECK]; MAX_DECKS],
            fx: None,
            cue_frames: [None; MAX_DECKS],
        }
    }
}

/// Translates controller events into application actions and application
/// state into LED feedback.
pub struct MidiBridge {
    audio: AudioController,
    library: Option<LibraryService>,
    pad_modes: PadModes,
    fader_start: bool,
    /// CH SELECT flags: channel 1 and channel 2.
    ch_select: [bool; 2],
    takeover: [Takeover; MAX_DECKS],
    cue_held: [bool; MAX_DECKS],
    pads: PadActions,
    taps: [Vec<u64>; MAX_DECKS],
    browse: BrowseCursor,
    view: View,
    sent: HashMap<FeedbackTarget, FeedbackValue>,
}

impl std::fmt::Debug for MidiBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiBridge").finish_non_exhaustive()
    }
}

impl MidiBridge {
    /// A bridge over `audio` (and the library, for browsing and loading).
    pub fn new(
        audio: AudioController,
        library: Option<LibraryService>,
        pad_modes: PadModes,
    ) -> Self {
        Self {
            pads: PadActions::new(audio.clone(), pad_modes.clone()),
            audio,
            library,
            pad_modes,
            fader_start: false,
            ch_select: [true, false],
            takeover: [Takeover::default(); MAX_DECKS],
            cue_held: [false; MAX_DECKS],
            taps: std::array::from_fn(|_| Vec::new()),
            browse: BrowseCursor::default(),
            view: View::default(),
            sent: HashMap::new(),
        }
    }

    /// Hardware fader start plays and pauses the deck.
    pub fn set_fader_start(&mut self, on: bool) {
        self.fader_start = on;
    }

    /// The list the library pane shows and its selected row.
    pub fn set_browse_context(&mut self, context: BrowseContextDto) {
        self.browse.source = context.source;
        self.browse.index = context.index;
    }

    /// The pane selected a row.
    pub fn browse_select(&mut self, index: u32) {
        self.browse.index = Some(index);
    }

    /// Forgets what was sent, so the next [`feedback`](Self::feedback) call
    /// returns every target (after a reconnect).
    pub fn reset_feedback(&mut self) {
        self.sent.clear();
    }

    /// Takes an engine or library event into account.
    pub fn observe(&mut self, event: &CoreEvent) {
        match event {
            CoreEvent::Telemetry(t) => {
                self.view.telemetry = **t;
                self.view.have_telemetry = true;
            }
            CoreEvent::Engine(EngineEvent::CuePointSet { deck, position }) => {
                self.view.cue_frames[deck.index()] = Some(position.0);
            }
            CoreEvent::TrackLoaded(loaded) => {
                self.takeover[loaded.deck.index()].reset();
                self.refresh_deck(loaded.deck);
            }
            CoreEvent::HotCues(cues) => {
                let slots = &mut self.view.hot_cues[cues.deck.index()];
                for (i, slot) in slots.iter_mut().enumerate() {
                    *slot = cues.slots.get(i).is_some_and(Option::is_some);
                }
            }
            CoreEvent::FxState(fx) => self.view.fx = Some((**fx).clone()),
            _ => {}
        }
    }

    /// Re-reads deck, mixer and FX state from the control thread.
    pub fn refresh(&mut self) {
        for deck in DeckId::ALL.iter().take(MVP_DECKS) {
            self.refresh_deck(*deck);
        }
        if let Ok(mixer) = self.audio.mixer_state() {
            self.view.mixer = Some(mixer);
        }
        if let Ok(fx) = self.audio.fx_state() {
            self.view.fx = Some(fx);
        }
    }

    /// The mixer as last read.
    pub fn mixer(&self) -> Option<&MixerState> {
        self.view.mixer.as_ref()
    }

    /// The deck states as last read (pad mode filled in).
    pub fn decks(&self) -> impl Iterator<Item = &DeckState> {
        self.view.decks.iter().flatten()
    }

    fn refresh_deck(&mut self, deck: DeckId) {
        if let Ok(mut state) = self.audio.deck_state(deck) {
            state.pad_mode = self.pad_modes.get(deck);
            let rate = f64::from(self.view.telemetry.sample_rate.0);
            self.view.cue_frames[deck.index()] = state
                .cue_point
                .map(|s| (s.0 * rate).max(0.0) as u64)
                .or(self.view.cue_frames[deck.index()]);
            self.view.decks[deck.index()] = Some(state);
        }
        if let Ok(cues) = self.audio.hot_cues(deck) {
            for (i, slot) in self.view.hot_cues[deck.index()].iter_mut().enumerate() {
                *slot = cues.slots.get(i).is_some_and(Option::is_some);
            }
        }
    }

    // ----- control events -----

    /// Applies one control event.
    pub fn handle(&mut self, event: &ControlEvent) -> Vec<BridgeNotice> {
        let mut notices = Vec::new();
        let pressed = matches!(event.value, ControlValue::Button(true));
        let released = matches!(event.value, ControlValue::Button(false));
        let button = pressed || released;
        let absolute = match event.value {
            ControlValue::Absolute(v) if v.is_finite() => Some(v.clamp(0.0, 1.0)),
            _ => None,
        };
        let relative = match event.value {
            ControlValue::Relative(v) if v.is_finite() => Some(v),
            _ => None,
        };
        let audio = self.audio.clone();
        let log = |what: &str, result: Result<(), crate::CoreError>| {
            if let Err(err) = result {
                tracing::debug!(%err, what, "MIDI action failed");
            }
        };
        match event.action {
            // ----- transport -----
            ControlAction::PlayPause { deck } if pressed => {
                log("play", audio.toggle_playing(deck));
            }
            ControlAction::Cue { deck } if button => {
                self.cue_held[deck.index()] = pressed;
                log("cue", audio.cue(deck, pressed));
            }
            ControlAction::ReturnToStart { deck } if pressed => {
                log("return", audio.seek(deck, Seconds(0.0)));
            }
            ControlAction::Sync { deck } if pressed => {
                let on = self.deck(deck).is_some_and(|d| d.sync != SyncMode::Off);
                let mode = if on { SyncMode::Off } else { SyncMode::Beat };
                log("sync", audio.set_sync(deck, mode));
            }
            ControlAction::SyncLeader { deck } if pressed => {
                log("leader", audio.set_leader(Some(deck)));
            }
            ControlAction::Load { deck } if pressed => self.load_selected(deck),

            // ----- tempo -----
            ControlAction::TempoFader { deck } => {
                if let Some(hw) = absolute {
                    let software = self
                        .fresh_deck(deck)
                        .map_or(0.5, |d| (d.tempo_fader.clamp(-1.0, 1.0) + 1.0) / 2.0);
                    if self.takeover[deck.index()].feed(hw, software) {
                        log("tempo", audio.set_tempo_fader(deck, hw * 2.0 - 1.0));
                    }
                }
            }
            ControlAction::TempoRangeNext { deck } if pressed => {
                if let Some(state) = self.fresh_deck(deck) {
                    log(
                        "range",
                        audio.set_tempo_range(deck, state.tempo_range.next()),
                    );
                    self.takeover[deck.index()].reset();
                }
            }
            ControlAction::KeyLock { deck } if pressed => {
                let on = self.fresh_deck(deck).is_some_and(|d| d.key_lock);
                log("key lock", audio.set_key_lock(deck, !on));
            }
            ControlAction::PitchBend { deck, direction } if button => {
                let amount = if pressed {
                    f32::from(direction.signum()) * PITCH_BEND_AMOUNT
                } else {
                    0.0
                };
                log("bend", audio.pitch_bend(deck, amount));
            }
            ControlAction::TapTempo { deck } if pressed => {
                self.tap_tempo(deck, event.timestamp_us);
            }

            // ----- jog -----
            ControlAction::JogTouch { deck } if button => {
                log("touch", audio.jog_touch(deck, pressed));
            }
            ControlAction::JogTop { deck } | ControlAction::JogSide { deck } => {
                if let Some(r) = relative {
                    log("jog", audio.jog_turn(deck, r));
                }
            }
            ControlAction::JogSearch { deck } => {
                if let Some(r) = relative {
                    log(
                        "search",
                        audio.jog_search(deck, Beats(f64::from(r) * SEARCH_BEATS_PER_REVOLUTION)),
                    );
                }
            }
            ControlAction::VinylMode { deck } if pressed => {
                let on = self.fresh_deck(deck).is_none_or(|d| d.vinyl_mode);
                log("vinyl", audio.set_vinyl_mode(deck, !on));
                if let Some(state) = self.view.decks[deck.index()].as_mut() {
                    state.vinyl_mode = !on;
                }
            }

            // ----- loops, jumps, cues -----
            ControlAction::LoopIn { deck } if pressed => log("loop in", audio.loop_in(deck)),
            ControlAction::LoopOut { deck } if pressed => log("loop out", audio.loop_out(deck)),
            ControlAction::LoopToggle { deck } if pressed => {
                log("loop toggle", audio.loop_toggle(deck));
            }
            ControlAction::AutoLoop { deck, length } if pressed => {
                log("auto loop", audio.auto_loop(deck, length));
            }
            ControlAction::LoopHalve { deck } if pressed => {
                log("loop halve", audio.resize_loop(deck, 0.5));
            }
            ControlAction::LoopDouble { deck } if pressed => {
                log("loop double", audio.resize_loop(deck, 2.0));
            }
            ControlAction::BeatJump { deck, beats } if pressed => {
                log("beat jump", audio.beat_jump(deck, beats));
            }
            ControlAction::CueLoopCallPrev { deck } if pressed => {
                log("call prev", audio.call_memory_cue(deck, -1));
            }
            ControlAction::CueLoopCallNext { deck } if pressed => {
                log("call next", audio.call_memory_cue(deck, 1));
            }
            ControlAction::MemoryCueAdd { deck } if pressed => {
                log("memory cue", audio.add_memory_cue(deck).map(|_| ()));
            }
            ControlAction::MemoryCueDelete { deck } if pressed => {
                log("memory cue delete", audio.delete_memory_cue(deck));
            }
            ControlAction::Slip { deck } if pressed => {
                let on = self.fresh_deck(deck).is_some_and(|d| d.slip);
                log("slip", audio.set_slip(deck, !on));
            }
            ControlAction::Quantize { deck } if pressed => {
                let on = self.fresh_deck(deck).is_none_or(|d| d.quantize);
                log("quantize", audio.set_quantize(deck, !on));
            }

            // ----- pads -----
            ControlAction::PadModeSelect { deck, mode } if pressed => {
                if self.pad_modes.set(deck, mode) {
                    notices.push(BridgeNotice::PadMode(PadModeChanged { deck, mode }));
                }
            }
            ControlAction::Pad {
                deck,
                mode,
                index,
                shifted,
            } if button => self.pad(deck, mode, index, shifted, pressed),
            ControlAction::HotCue { deck, index } if button => {
                self.pads.hot_cue(deck, index.index() as u8, false, pressed);
            }
            ControlAction::HotCueDelete { deck, index } if pressed => {
                log("hot cue delete", audio.delete_hot_cue(deck, index));
            }

            // ----- mixer -----
            ControlAction::Trim { deck } => {
                if let Some(v) = absolute {
                    log("trim", audio.set_trim(deck, trim_gain(v)));
                }
            }
            ControlAction::Eq { deck, band } => {
                if let Some(v) = absolute {
                    log("eq", audio.set_eq(deck, band, eq_gain(v)));
                }
            }
            ControlAction::Filter { deck } => {
                // The FLX4 CFX knob is the Sound Color FX of the channel.
                if let Some(v) = absolute {
                    log(
                        "cfx",
                        audio.fx_apply(FxOp::SetCfx {
                            deck,
                            amount: v * 2.0 - 1.0,
                        }),
                    );
                }
            }
            ControlAction::ChannelFader { deck } => {
                if let Some(v) = absolute {
                    log("fader", audio.set_channel_fader(deck, v));
                }
            }
            ControlAction::ChannelFaderStart { deck } if button && self.fader_start => {
                log("fader start", audio.set_playing(deck, pressed));
            }
            ControlAction::Pfl { deck } if pressed => {
                let on = audio
                    .mixer_state()
                    .map(|m| m.channels[deck.index()].pfl)
                    .unwrap_or(false);
                log("pfl", audio.set_pfl(deck, !on));
            }
            ControlAction::Crossfader => {
                if let Some(v) = absolute {
                    log("crossfader", audio.set_crossfader(v * 2.0 - 1.0));
                }
            }
            ControlAction::MasterLevel => {
                if let Some(v) = absolute {
                    log("master", audio.set_master_level(level_gain(v)));
                }
            }
            ControlAction::HeadphoneLevel => {
                if let Some(v) = absolute {
                    log("headphones", audio.set_headphone_level(level_gain(v)));
                }
            }
            ControlAction::HeadphoneMix => {
                if let Some(v) = absolute {
                    log("headphone mix", audio.set_headphone_mix(v));
                }
            }
            // TODO(M7): MASTER CUE has no engine concept yet (monitor master
            // in the headphones); the press is dropped.
            ControlAction::MasterCue => {}

            // ----- Beat FX -----
            ControlAction::FxAssign { target, .. } if button => {
                if let Some(deck) = target.filter(|d| d.index() < 2) {
                    self.ch_select[deck.index()] = pressed;
                }
            }
            ControlAction::FxToggle { .. } if pressed => {
                let Some(fx) = self.fresh_fx() else {
                    return notices;
                };
                for unit in self.section_units(&fx) {
                    let on = fx.units[usize::from(unit)].on;
                    log("fx on", audio.fx_apply(FxOp::SetUnitOn { unit, on: !on }));
                }
            }
            ControlAction::FxRelease { .. } if pressed => {
                let Some(fx) = self.fresh_fx() else {
                    return notices;
                };
                for unit in self.section_units(&fx) {
                    log("fx release", audio.fx_apply(FxOp::Release { unit }));
                }
            }
            ControlAction::FxLevel { .. } => {
                if let Some(v) = absolute {
                    let Some(fx) = self.fresh_fx() else {
                        return notices;
                    };
                    for unit in self.section_units(&fx) {
                        for slot in 0..crate::fx_types::FX_CHAIN_SLOTS {
                            if fx.units[usize::from(unit)].slots[slot].effect.is_some() {
                                log(
                                    "fx level",
                                    audio.fx_apply(FxOp::SetWet {
                                        slot: slot_ref(unit, slot as u8),
                                        wet: v,
                                    }),
                                );
                            }
                        }
                    }
                }
            }
            ControlAction::FxSelect { direction, .. } if pressed => self.fx_select(direction),
            ControlAction::FxBeat { direction, .. } if pressed => {
                let Some(fx) = self.fresh_fx() else {
                    return notices;
                };
                for unit in self.section_units(&fx) {
                    log(
                        "fx beat",
                        audio.fx_apply(FxOp::StepDivision {
                            unit: FxUnitRef {
                                target: FxTarget::Beat,
                                unit,
                            },
                            direction,
                        }),
                    );
                }
            }
            ControlAction::FxBpmAuto { .. } if pressed => {
                let Some(fx) = self.fresh_fx() else {
                    return notices;
                };
                for unit in self.section_units(&fx) {
                    log(
                        "fx bpm auto",
                        audio.fx_apply(FxOp::SetBpmSource {
                            unit,
                            source: FxBpmSource::Auto,
                        }),
                    );
                }
            }
            ControlAction::FxBpmTap { .. } if pressed => {
                let Some(fx) = self.fresh_fx() else {
                    return notices;
                };
                for unit in self.section_units(&fx) {
                    log("fx tap", audio.fx_apply(FxOp::Tap { unit }));
                }
            }

            // ----- browser -----
            ControlAction::BrowseScroll => {
                if let (Some(r), Some(library)) = (relative, self.library.clone()) {
                    let rows = r.round() as i64;
                    let rows = if rows == 0 && r != 0.0 {
                        r.signum() as i64
                    } else {
                        rows
                    };
                    if let Some(cursor) = self.browse.step(&library, rows * i64::from(BROWSE_PAGE))
                    {
                        notices.push(BridgeNotice::Browse(cursor));
                    }
                }
            }
            // Sampler pads, browse press and back, and the rest have no
            // application action yet.
            _ => {}
        }
        if pressed {
            self.refresh();
        }
        notices
    }

    fn deck(&self, deck: DeckId) -> Option<&DeckState> {
        self.view.decks[deck.index()].as_ref()
    }

    fn fresh_deck(&mut self, deck: DeckId) -> Option<DeckState> {
        self.refresh_deck(deck);
        self.deck(deck).cloned()
    }

    fn fresh_fx(&mut self) -> Option<FxStateDto> {
        if let Ok(fx) = self.audio.fx_state() {
            self.view.fx = Some(fx);
        }
        self.view.fx.clone()
    }

    /// The Beat FX units the hardware section currently controls: those that
    /// own a channel CH SELECT has set.
    fn section_units(&self, fx: &FxStateDto) -> Vec<u8> {
        let mut units = Vec::new();
        for (channel, selected) in self.ch_select.iter().enumerate() {
            if !selected {
                continue;
            }
            let owner = fx
                .units
                .iter()
                .position(|u| owns(u.assign, channel))
                .and_then(|i| u8::try_from(i).ok());
            if let Some(unit) = owner
                && !units.contains(&unit)
            {
                units.push(unit);
            }
        }
        units
    }

    fn fx_select(&mut self, direction: i8) {
        let Some(fx) = self.fresh_fx() else { return };
        let Ok(effects) = self.audio.fx_effects() else {
            return;
        };
        let candidates: Vec<_> = effects
            .iter()
            .filter(|e| e.routing == opendeck_contracts::fx::FxRouting::Insert && !e.release)
            .map(|e| e.key.clone())
            .collect();
        if candidates.is_empty() {
            return;
        }
        let len = candidates.len() as i64;
        for unit in self.section_units(&fx) {
            let current = fx.units[usize::from(unit)].slots[0].effect.as_ref();
            let at = current
                .and_then(|c| candidates.iter().position(|k| k == c))
                .map_or(if direction >= 0 { -1 } else { 0 }, |i| i as i64);
            let next = (at + i64::from(direction.signum())).rem_euclid(len);
            let _ = self.audio.fx_apply(FxOp::SetEffect {
                slot: slot_ref(unit, 0),
                effect: Some(candidates[next as usize].clone()),
            });
        }
    }

    fn load_selected(&mut self, deck: DeckId) {
        let Some(library) = self.library.clone() else {
            return;
        };
        let Some(track) = self.browse.selected(&library) else {
            return;
        };
        self.takeover[deck.index()].reset();
        if let Err(err) = library.load_track(deck, track) {
            tracing::debug!(%err, "MIDI load failed");
        }
    }

    /// Tap tempo: the mean interval of the taps since the last pause sets the
    /// deck's tempo through its fader.
    fn tap_tempo(&mut self, deck: DeckId, now_us: u64) {
        let taps = &mut self.taps[deck.index()];
        if taps
            .last()
            .is_some_and(|last| now_us.saturating_sub(*last) > TAP_RESET_US)
        {
            taps.clear();
        }
        taps.push(now_us);
        if taps.len() > 8 {
            taps.remove(0);
        }
        if taps.len() < 2 {
            return;
        }
        let span = taps[taps.len() - 1] - taps[0];
        let interval_s = span as f64 / 1e6 / (taps.len() - 1) as f64;
        if interval_s <= 0.0 {
            return;
        }
        let tapped_bpm = 60.0 / interval_s;
        let Some(state) = self.fresh_deck(deck) else {
            return;
        };
        let Some(track_bpm) = state.track_bpm else {
            return;
        };
        if track_bpm.0 <= 0.0 {
            return;
        }
        let fraction = state.tempo_range.max_fraction();
        let position = ((tapped_bpm / track_bpm.0 - 1.0) / fraction).clamp(-1.0, 1.0);
        let _ = self.audio.set_tempo_fader(deck, position as f32);
        self.takeover[deck.index()].reset();
    }

    fn pad(&mut self, deck: DeckId, mode: PadMode, index: u8, shifted: bool, pressed: bool) {
        let browse = self
            .library
            .as_ref()
            .and_then(|library| self.browse.selected(library));
        self.pads.press(deck, mode, index, shifted, pressed, browse);
    }

    // ----- feedback -----

    /// LED and meter states that changed since the last call.
    pub fn feedback(&mut self) -> Vec<(FeedbackTarget, FeedbackValue)> {
        let mut changes = Vec::new();
        let sent = &mut self.sent;
        let mut put = |target: FeedbackTarget, value: FeedbackValue| {
            if sent.get(&target) != Some(&value) {
                sent.insert(target, value);
                changes.push((target, value));
            }
        };
        let light = |on: bool| {
            if on {
                FeedbackValue::On
            } else {
                FeedbackValue::Off
            }
        };
        let t = &self.view.telemetry;
        let rate = f64::from(t.sample_rate.0);
        for deck in DeckId::ALL.iter().take(MVP_DECKS).copied() {
            let i = deck.index();
            let dt = &t.decks[i];
            let state = self.view.decks[i].as_ref();
            let loaded = dt.loaded;

            let playing = if dt.playing {
                FeedbackValue::On
            } else if loaded {
                FeedbackValue::Blink
            } else {
                FeedbackValue::Off
            };
            put(FeedbackTarget::Playing { deck }, playing);

            let at_cue = !dt.playing
                && loaded
                && self.view.cue_frames[i].is_some_and(|c| {
                    (dt.position.0 as i64 - c as i64).unsigned_abs() as f64 <= rate / 50.0
                });
            put(
                FeedbackTarget::CueIndicator { deck },
                light(self.cue_held[i] || at_cue),
            );
            put(
                FeedbackTarget::SyncOn { deck },
                light(dt.sync_mode != SyncMode::Off),
            );
            put(FeedbackTarget::LoopActive { deck }, light(dt.loop_active));
            put(
                FeedbackTarget::PflOn { deck },
                light(self.view.mixer.as_ref().is_some_and(|m| m.channels[i].pfl)),
            );
            put(FeedbackTarget::TrackLoaded { deck }, light(loaded));
            put(
                FeedbackTarget::VinylMode { deck },
                light(state.is_none_or(|s| s.vinyl_mode)),
            );
            let peak = dt.meter.peak_l.max(dt.meter.peak_r);
            put(
                FeedbackTarget::ChannelLevel { deck },
                FeedbackValue::Level(quantize_level(meter_level(peak))),
            );

            let active = self.pad_modes.get(deck);
            for mode in PadMode::ALL {
                put(
                    FeedbackTarget::PadModeActive { deck, mode },
                    light(mode == active),
                );
                for pad in 0..PADS_PER_LAYER as u8 {
                    let on = mode == active
                        && pad_lit(&self.view, &self.pad_modes, deck, mode, pad, state);
                    put(
                        FeedbackTarget::Pad {
                            deck,
                            mode,
                            index: pad,
                        },
                        light(on),
                    );
                }
            }
        }
        for n in 0..2u8 {
            let Some(deck) = DeckId::new(n) else { continue };
            let on = self.view.fx.as_ref().is_some_and(|fx| {
                fx.units
                    .iter()
                    .any(|u| owns(u.assign, usize::from(n)) && u.on)
            });
            put(
                FeedbackTarget::FxOn {
                    unit: FxUnit::Deck(deck),
                },
                if on {
                    FeedbackValue::Blink
                } else {
                    FeedbackValue::Off
                },
            );
        }
        changes
    }
}

fn pad_lit(
    view: &View,
    pads: &PadModes,
    deck: DeckId,
    mode: PadMode,
    pad: u8,
    state: Option<&DeckState>,
) -> bool {
    let i = deck.index();
    match mode {
        PadMode::HotCue => view.hot_cues[i][usize::from(pad)],
        PadMode::PadFx1 | PadMode::PadFx2 => {
            let bank = usize::from(mode == PadMode::PadFx2);
            view.fx
                .as_ref()
                .and_then(|fx| fx.pads.get(bank * 2 * PADS_PER_LAYER + usize::from(pad)))
                .is_some_and(|p| p.effect.is_some())
        }
        PadMode::BeatJump => view.telemetry.decks[i].loaded,
        PadMode::BeatLoop => {
            let dt = &view.telemetry.decks[i];
            let (Some(region), true) = (dt.loop_region, dt.loop_active) else {
                return false;
            };
            let Some(end) = region.end else { return false };
            let bpm = state.and_then(|s| s.track_bpm).map_or(0.0, |b| b.0);
            if bpm <= 0.0 {
                return false;
            }
            let frames_per_beat = f64::from(view.telemetry.sample_rate.0) * 60.0 / bpm;
            let beats = (end.0.saturating_sub(region.start.0)) as f64 / frames_per_beat;
            (beats - BEAT_LOOP_LENGTHS[usize::from(pad)]).abs() < 0.02
        }
        PadMode::Sampler => pads
            .sampler()
            .slot_for_pad(deck, pad)
            .is_some_and(|slot| pads.sampler().loaded_mask()[slot.index()]),
        // Before a cue is chosen the pads show which cues exist; then every
        // pad is a note.
        PadMode::Keyboard => {
            pads.keyboard_cue(deck).is_some() || view.hot_cues[i][usize::from(pad)]
        }
        // The pad whose pitch is the deck's current key shift.
        PadMode::KeyShift => {
            state.is_some_and(|s| pad_semitones(pad, pads.key_offset(deck)) == s.key_shift)
        }
    }
}

/// Meter values are sent in 1/128 steps so a steady signal sends nothing.
fn quantize_level(level: f32) -> f32 {
    (level * 128.0).round() / 128.0
}

fn owns(assign: FxAssign, channel: usize) -> bool {
    match assign {
        FxAssign::None => false,
        FxAssign::Ch1 => channel == 0,
        FxAssign::Ch2 => channel == 1,
        FxAssign::Both => channel < 2,
    }
}

fn slot_ref(unit: u8, slot: u8) -> FxSlotRef {
    FxSlotRef {
        unit: FxUnitRef {
            target: FxTarget::Beat,
            unit,
        },
        slot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takeover_waits_for_the_software_value() {
        let mut t = Takeover::default();
        assert!(!t.feed(0.9, 0.5), "far from the software value: ignored");
        assert!(!t.feed(0.8, 0.5));
        assert!(t.feed(0.51, 0.5), "inside the zone: picked up");
        assert!(t.feed(0.7, 0.5), "follows after pickup");
    }

    #[test]
    fn takeover_picks_up_on_crossing() {
        let mut t = Takeover::default();
        assert!(!t.feed(0.9, 0.5));
        assert!(t.feed(0.3, 0.5), "crossed the software value in one step");
    }

    #[test]
    fn takeover_releases_when_the_software_value_moves() {
        let mut t = Takeover::default();
        assert!(t.feed(0.5, 0.5));
        assert!(!t.feed(0.6, 0.9), "the UI moved the value: wait again");
        t.reset();
        assert!(!t.feed(0.1, 0.9));
    }

    #[test]
    fn curves_match_the_ui() {
        assert!((trim_gain(0.5).0 - 1.0).abs() < 1e-6);
        assert_eq!(eq_gain(0.0).0, 0.0);
        assert!((eq_gain(0.5).0 - 1.0).abs() < 1e-6);
        assert!((eq_gain(1.0).0 - db_to_gain(6.0)).abs() < 1e-5);
        assert!((level_gain(0.75).0 - 1.0).abs() < 1e-6);
        assert_eq!(level_gain(0.0).0, 0.0);
    }

    #[test]
    fn meter_curve_is_monotonic_and_clamped() {
        assert_eq!(meter_level(0.0), 0.0);
        assert!((meter_level(1.0) - 1.0).abs() < 1e-6);
        assert!(meter_level(0.5) > meter_level(0.1));
        assert_eq!(meter_level(1e-6), 0.0);
    }

    #[test]
    fn pad_modes_are_shared_between_clones() {
        let modes = PadModes::default();
        let other = modes.clone();
        let deck = DeckId::new(1).expect("deck 2");
        assert!(modes.set(deck, PadMode::Sampler));
        assert_eq!(other.get(deck), PadMode::Sampler);
        assert!(!other.set(deck, PadMode::Sampler));
    }

    #[test]
    fn keyboard_pads_show_cues_then_light_as_notes() {
        let mut view = View::default();
        view.hot_cues[0][2] = true;
        let pads = PadModes::default();
        let deck = DeckId::new(0).expect("deck");
        let lit = |pads: &PadModes, pad| pad_lit(&view, pads, deck, PadMode::Keyboard, pad, None);
        assert!(lit(&pads, 2));
        assert!(!lit(&pads, 3));
        pads.set_keyboard_cue(deck, Some(2));
        assert!(lit(&pads, 3), "every pad is a note once a cue is chosen");
    }
}
