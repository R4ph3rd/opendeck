//! Performance pads: shared per-deck pad state and the interpretation of a
//! pad press in each of the eight modes (milestone 7).
//!
//! The MIDI bridge and the UI both call [`PadActions::press`], so a pad on the
//! controller and a pad on screen do the same thing. [`PadModes`] is the
//! state both sides share: the active mode of each deck, the beat jump and
//! pitch offsets changed with SHIFT, the hot cue chosen in KEYBOARD mode and
//! the sampler.
//!
//! Pad layouts follow the DDJ-FLX4 manual (pages are of the French PDF):
//!
//! - HOT CUE (p.57, hot cue section): set, jump, SHIFT deletes.
//! - PAD FX 1 and 2 (p.58-59): hold to engage, release to stop; the 32 pad FX
//!   slots are `bank * 16 + shift * 8 + pad` in the FX model.
//! - BEAT JUMP (p.60): pads 1-8 jump 1, 1, 2, 2, 4, 4, 8, 8 beats alternately
//!   left and right. SHIFT + pad 7 halves and SHIFT + pad 8 doubles the sizes.
//! - SAMPLER (p.61-62): pad press plays a slot, SHIFT + pad stops it, or loads
//!   the track selected in the browser into an empty slot.
//! - KEYBOARD (p.63-64): the first pad press picks the hot cue (empty slots
//!   are set at the playhead); then the pads play it from +4 to +7 over
//!   ±0 to +3 semitones. SHIFT + pads 1-6 delete the cue, SHIFT + pad 7 and 8
//!   move the pitch up and down.
//! - BEAT LOOP (p.64): 1/4 to 32 beats; the same pad again exits the loop.
//! - KEY SHIFT (p.65): the same semitone layout as KEYBOARD, applied to the
//!   deck's key; SHIFT + pad 7 and 8 move the layout up and down.

use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender};
use opendeck_contracts::cue::HotCueIndex;
use opendeck_contracts::ids::TrackId;
use opendeck_contracts::pad::PadMode;
use opendeck_contracts::{Beats, DeckId, EngineCommand, HOT_CUES_PER_DECK, MAX_DECKS, MVP_DECKS};
use serde::{Deserialize, Serialize};

use crate::AudioController;
use crate::fx_types::PADS_PER_LAYER;
use crate::midi_types::PadModeChanged;
use crate::sampler::{SamplerService, SamplerStateDto};

/// Pad index to auto loop length in BEAT LOOP mode (beats), manual p.64.
pub const BEAT_LOOP_LENGTHS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];

/// Beat jump sizes of pad pairs (left, right) before SHIFT scaling, p.60.
pub const BEAT_JUMP_BASE: [f64; 4] = [1.0, 2.0, 4.0, 8.0];

/// Beat jump scale steps: sizes are multiplied by `2^step`.
pub const JUMP_STEP_RANGE: (i8, i8) = (-2, 2);

/// Semitones of the pads before the offset, KEYBOARD and KEY SHIFT (p.63, p.65).
pub const PAD_SEMITONES: [i8; 8] = [4, 5, 6, 7, 0, 1, 2, 3];

/// Largest key shift the engine accepts.
pub const MAX_SEMITONES: i8 = 12;

/// Signed beat jump of `pad` with the deck's scale `step`.
pub fn beat_jump_beats(pad: u8, step: i8) -> f64 {
    let base = BEAT_JUMP_BASE[usize::from(pad / 2) % 4] * 2f64.powi(i32::from(step));
    if pad.is_multiple_of(2) { -base } else { base }
}

/// Semitones of `pad` with the deck's pitch `offset`, within the engine range.
pub fn pad_semitones(pad: u8, offset: i8) -> i8 {
    (PAD_SEMITONES[usize::from(pad) % 8] + offset).clamp(-MAX_SEMITONES, MAX_SEMITONES)
}

/// Pad state of one deck, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeckPadStateDto {
    /// Deck.
    pub deck: DeckId,
    /// Active pad mode.
    pub mode: PadMode,
    /// Signed beats of the eight BEAT JUMP pads.
    pub jump_beats: Vec<f64>,
    /// Loop beats of the eight BEAT LOOP pads.
    pub loop_beats: Vec<f64>,
    /// Semitones of the eight KEY SHIFT pads.
    pub key_semitones: Vec<i8>,
    /// Hot cue chosen in KEYBOARD mode.
    pub keyboard_cue: Option<u8>,
    /// Semitones of the eight KEYBOARD pads.
    pub keyboard_semitones: Vec<i8>,
}

/// Pad state of every shown deck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PadStateDto {
    /// One entry per shown deck.
    pub decks: Vec<DeckPadStateDto>,
}

/// Something the shell forwards to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum PadEvent {
    /// The mode of a deck was changed from the UI (the controller's own mode
    /// buttons are reported by the MIDI service).
    Mode(PadModeChanged),
    /// Beat jump, pitch or keyboard cue changed.
    State(PadStateDto),
    /// The sampler changed.
    Sampler(Box<SamplerStateDto>),
}

/// Subscribers of [`PadEvent`]s.
#[derive(Debug, Clone, Default)]
pub(crate) struct Notifier(Arc<Mutex<Vec<Sender<PadEvent>>>>);

impl Notifier {
    pub(crate) fn send(&self, event: PadEvent) {
        let mut subs = self.0.lock().unwrap_or_else(|e| e.into_inner());
        subs.retain(|s| s.send(event.clone()).is_ok());
    }

    fn subscribe(&self) -> Receiver<PadEvent> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(tx);
        rx
    }
}

#[derive(Debug, Clone, Copy)]
struct DeckPads {
    mode: PadMode,
    jump_step: i8,
    key_offset: i8,
    keyboard_offset: i8,
    keyboard_cue: Option<u8>,
}

impl Default for DeckPads {
    fn default() -> Self {
        Self {
            mode: PadMode::HotCue,
            jump_step: 0,
            key_offset: 0,
            keyboard_offset: 0,
            keyboard_cue: None,
        }
    }
}

/// Pad state of every deck, shared between the shell and the MIDI service.
#[derive(Debug, Clone)]
pub struct PadModes {
    decks: Arc<Mutex<[DeckPads; MAX_DECKS]>>,
    notifier: Notifier,
    sampler: SamplerService,
}

impl Default for PadModes {
    fn default() -> Self {
        let notifier = Notifier::default();
        Self {
            decks: Arc::new(Mutex::new([DeckPads::default(); MAX_DECKS])),
            sampler: SamplerService::new(notifier.clone()),
            notifier,
        }
    }
}

impl PadModes {
    fn lock(&self) -> std::sync::MutexGuard<'_, [DeckPads; MAX_DECKS]> {
        self.decks.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Active mode of `deck`.
    pub fn get(&self, deck: DeckId) -> PadMode {
        self.lock()[deck.index()].mode
    }

    /// Sets the mode; true if it changed. Choosing KEYBOARD forgets the hot
    /// cue, so the next pad press picks one again (manual p.63).
    pub fn set(&self, deck: DeckId, mode: PadMode) -> bool {
        let (changed, forgot) = {
            let mut decks = self.lock();
            let d = &mut decks[deck.index()];
            let changed = d.mode != mode;
            d.mode = mode;
            let forgot = mode == PadMode::Keyboard && d.keyboard_cue.take().is_some();
            (changed, forgot)
        };
        if forgot {
            self.publish();
        }
        changed
    }

    /// Like [`set`](Self::set), and tells subscribers when the mode changed.
    /// For changes that start in the UI.
    pub fn select(&self, deck: DeckId, mode: PadMode) -> bool {
        let changed = self.set(deck, mode);
        if changed {
            self.notifier
                .send(PadEvent::Mode(PadModeChanged { deck, mode }));
        }
        changed
    }

    /// The sampler shared by every deck.
    pub fn sampler(&self) -> SamplerService {
        self.sampler.clone()
    }

    /// Events for the UI: pad state and sampler changes.
    pub fn subscribe(&self) -> Receiver<PadEvent> {
        self.notifier.subscribe()
    }

    /// Beat jump scale step of `deck` (sizes are `base * 2^step`).
    pub fn jump_step(&self, deck: DeckId) -> i8 {
        self.lock()[deck.index()].jump_step
    }

    /// Pitch offset of KEY SHIFT pads on `deck`.
    pub fn key_offset(&self, deck: DeckId) -> i8 {
        self.lock()[deck.index()].key_offset
    }

    /// Pitch offset of KEYBOARD pads on `deck`.
    pub fn keyboard_offset(&self, deck: DeckId) -> i8 {
        self.lock()[deck.index()].keyboard_offset
    }

    /// Hot cue chosen in KEYBOARD mode.
    pub fn keyboard_cue(&self, deck: DeckId) -> Option<u8> {
        self.lock()[deck.index()].keyboard_cue
    }

    /// Chooses (or forgets) the KEYBOARD hot cue.
    pub fn set_keyboard_cue(&self, deck: DeckId, cue: Option<u8>) {
        self.lock()[deck.index()].keyboard_cue = cue;
        self.publish();
    }

    /// Moves the beat jump scale by `delta` steps; returns the new step.
    pub fn adjust_jump(&self, deck: DeckId, delta: i8) -> i8 {
        let step = {
            let mut decks = self.lock();
            let d = &mut decks[deck.index()];
            d.jump_step = (d.jump_step + delta).clamp(JUMP_STEP_RANGE.0, JUMP_STEP_RANGE.1);
            d.jump_step
        };
        self.publish();
        step
    }

    /// Moves the KEY SHIFT layout by `delta` semitones.
    pub fn adjust_key(&self, deck: DeckId, delta: i8) {
        {
            let mut decks = self.lock();
            let d = &mut decks[deck.index()];
            d.key_offset = (d.key_offset + delta).clamp(-MAX_SEMITONES, MAX_SEMITONES);
        }
        self.publish();
    }

    /// Moves the KEYBOARD layout by `delta` semitones.
    pub fn adjust_keyboard(&self, deck: DeckId, delta: i8) {
        {
            let mut decks = self.lock();
            let d = &mut decks[deck.index()];
            d.keyboard_offset = (d.keyboard_offset + delta).clamp(-MAX_SEMITONES, MAX_SEMITONES);
        }
        self.publish();
    }

    /// Pad state of every shown deck.
    pub fn state(&self) -> PadStateDto {
        let decks = self.lock();
        PadStateDto {
            decks: DeckId::ALL
                .iter()
                .take(MVP_DECKS)
                .map(|deck| {
                    let d = decks[deck.index()];
                    DeckPadStateDto {
                        deck: *deck,
                        mode: d.mode,
                        jump_beats: (0..8).map(|p| beat_jump_beats(p, d.jump_step)).collect(),
                        loop_beats: BEAT_LOOP_LENGTHS.to_vec(),
                        key_semitones: (0..8).map(|p| pad_semitones(p, d.key_offset)).collect(),
                        keyboard_cue: d.keyboard_cue,
                        keyboard_semitones: (0..8)
                            .map(|p| pad_semitones(p, d.keyboard_offset))
                            .collect(),
                    }
                })
                .collect(),
        }
    }

    fn publish(&self) {
        self.notifier.send(PadEvent::State(self.state()));
    }
}

/// What the interpreter knows about the deck when a pad is pressed.
#[derive(Debug, Clone, Default)]
pub struct PadContext {
    /// Which hot cue slots hold a cue.
    pub hot_cue_set: [bool; HOT_CUES_PER_DECK],
    /// Length in beats of the active loop, if looping.
    pub loop_beats: Option<f64>,
    /// Track selected in the browser (SAMPLER load).
    pub browse_track: Option<TrackId>,
    /// Which sampler slots hold a sample.
    pub sampler_loaded: [bool; opendeck_contracts::ids::SAMPLER_SLOTS],
}

/// One thing a pad press makes happen outside the pad state.
#[derive(Debug, Clone, PartialEq)]
pub enum PadOp {
    /// An engine command.
    Engine(EngineCommand),
    /// Hot cue edge on a set cue.
    HotCue {
        /// Deck.
        deck: DeckId,
        /// Slot.
        index: HotCueIndex,
        /// Press or release.
        pressed: bool,
    },
    /// Store the playhead in a slot.
    SetHotCue {
        /// Deck.
        deck: DeckId,
        /// Slot.
        index: HotCueIndex,
    },
    /// Clear a slot.
    DeleteHotCue {
        /// Deck.
        deck: DeckId,
        /// Slot.
        index: HotCueIndex,
    },
    /// Pad FX edge.
    FxPad {
        /// Deck.
        deck: DeckId,
        /// Bank 0 or 1.
        bank: u8,
        /// SHIFT layer.
        shift: bool,
        /// Pad.
        pad: u8,
        /// Press or release.
        pressed: bool,
    },
    /// Sampler edge.
    Sampler {
        /// Slot.
        slot: opendeck_contracts::SamplerSlotId,
        /// Press or release.
        pressed: bool,
    },
    /// Stop a sampler slot.
    SamplerStop(opendeck_contracts::SamplerSlotId),
    /// Load a library track into a sampler slot.
    SamplerLoad {
        /// Slot.
        slot: opendeck_contracts::SamplerSlotId,
        /// Track.
        track: TrackId,
    },
}

/// Pad interpreter. Cloning shares the held-pad memory.
#[derive(Clone)]
pub struct PadActions {
    audio: AudioController,
    pads: PadModes,
    /// Pads whose press started a hot cue, so the release stops it.
    held: Arc<Mutex<[[bool; PADS_PER_LAYER]; MAX_DECKS]>>,
}

impl std::fmt::Debug for PadActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PadActions").finish_non_exhaustive()
    }
}

impl PadActions {
    /// An interpreter sending to `audio` and keeping state in `pads`.
    pub fn new(audio: AudioController, pads: PadModes) -> Self {
        Self {
            audio,
            pads,
            held: Arc::new(Mutex::new([[false; PADS_PER_LAYER]; MAX_DECKS])),
        }
    }

    /// The shared pad state.
    pub fn pads(&self) -> &PadModes {
        &self.pads
    }

    /// A pad edge. Ignored unless `mode` is the deck's active mode, or `pad`
    /// is out of range. `browse_track` is the track selected in the browser.
    pub fn press(
        &self,
        deck: DeckId,
        mode: PadMode,
        pad: u8,
        shifted: bool,
        pressed: bool,
        browse_track: Option<TrackId>,
    ) {
        if mode != self.pads.get(deck) {
            return;
        }
        self.dispatch(deck, mode, pad, shifted, pressed, browse_track);
    }

    fn dispatch(
        &self,
        deck: DeckId,
        mode: PadMode,
        pad: u8,
        shifted: bool,
        pressed: bool,
        browse_track: Option<TrackId>,
    ) {
        if usize::from(pad) >= PADS_PER_LAYER {
            return;
        }
        let mut ctx = PadContext {
            browse_track,
            sampler_loaded: self.pads.sampler().loaded_mask(),
            ..PadContext::default()
        };
        match mode {
            PadMode::HotCue | PadMode::Keyboard if pressed => {
                if let Ok(cues) = self.audio.hot_cues(deck) {
                    for (i, slot) in ctx.hot_cue_set.iter_mut().enumerate() {
                        *slot = cues.slots.get(i).is_some_and(Option::is_some);
                    }
                }
            }
            PadMode::BeatLoop if pressed => {
                ctx.loop_beats = self.audio.deck_state(deck).ok().and_then(|s| {
                    let bpm = s.track_bpm?.0;
                    let l = s.active_loop?;
                    (bpm > 0.0).then(|| (l.end.0 - l.start.0) * bpm / 60.0)
                });
            }
            _ => {}
        }
        let held = {
            let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
            let slot = &mut held[deck.index()][usize::from(pad)];
            std::mem::take(slot)
        };
        let (ops, started) = plan(&self.pads, deck, mode, pad, shifted, pressed, held, &ctx);
        if started {
            self.held.lock().unwrap_or_else(|e| e.into_inner())[deck.index()][usize::from(pad)] =
                true;
        }
        for op in ops {
            self.run(op);
        }
    }

    /// Hot cue pad edge without a mode check (the dedicated HOT CUE controls).
    pub fn hot_cue(&self, deck: DeckId, pad: u8, shifted: bool, pressed: bool) {
        self.dispatch(deck, PadMode::HotCue, pad, shifted, pressed, None);
    }

    fn run(&self, op: PadOp) {
        let result = match op {
            PadOp::Engine(command) => self.audio.send(command),
            PadOp::HotCue {
                deck,
                index,
                pressed,
            } => self.audio.trigger_hot_cue(deck, index, pressed),
            PadOp::SetHotCue { deck, index } => self.audio.set_hot_cue(deck, index).map(|_| ()),
            PadOp::DeleteHotCue { deck, index } => self.audio.delete_hot_cue(deck, index),
            PadOp::FxPad {
                deck,
                bank,
                shift,
                pad,
                pressed,
            } => self.audio.fx_pad(deck, bank, shift, pad, pressed),
            PadOp::Sampler { slot, pressed } => self.pads.sampler().trigger(slot, pressed),
            PadOp::SamplerStop(slot) => self.pads.sampler().stop(slot),
            PadOp::SamplerLoad { slot, track } => {
                self.pads.sampler().load_track(slot.index() as u8, track)
            }
        };
        if let Err(err) = result {
            tracing::debug!(%err, "pad action failed");
        }
    }
}

/// Interprets one pad edge. Pure apart from the pad state it adjusts; returns
/// the operations to run and whether this press started a hot cue (so its
/// release must stop it). `was_held` is that flag from the earlier press.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    pads: &PadModes,
    deck: DeckId,
    mode: PadMode,
    pad: u8,
    shifted: bool,
    pressed: bool,
    was_held: bool,
    ctx: &PadContext,
) -> (Vec<PadOp>, bool) {
    let mut ops = Vec::new();
    let mut started = false;
    let cue = HotCueIndex::new(pad);
    match mode {
        PadMode::HotCue => {
            let Some(index) = cue else {
                return (ops, false);
            };
            if shifted {
                if pressed {
                    ops.push(PadOp::DeleteHotCue { deck, index });
                }
            } else if pressed {
                if ctx.hot_cue_set[index.index()] {
                    started = true;
                    ops.push(PadOp::HotCue {
                        deck,
                        index,
                        pressed: true,
                    });
                } else {
                    ops.push(PadOp::SetHotCue { deck, index });
                }
            } else if was_held {
                ops.push(PadOp::HotCue {
                    deck,
                    index,
                    pressed: false,
                });
            }
        }
        PadMode::PadFx1 | PadMode::PadFx2 => ops.push(PadOp::FxPad {
            deck,
            bank: u8::from(mode == PadMode::PadFx2),
            shift: shifted,
            pad,
            pressed,
        }),
        PadMode::BeatJump => {
            if pressed {
                if shifted {
                    match pad {
                        6 => {
                            pads.adjust_jump(deck, -1);
                        }
                        7 => {
                            pads.adjust_jump(deck, 1);
                        }
                        _ => {}
                    }
                } else {
                    ops.push(PadOp::Engine(EngineCommand::BeatJump {
                        deck,
                        beats: Beats(beat_jump_beats(pad, pads.jump_step(deck))),
                    }));
                }
            }
        }
        PadMode::BeatLoop => {
            if pressed && !shifted {
                let length = BEAT_LOOP_LENGTHS[usize::from(pad)];
                if ctx
                    .loop_beats
                    .is_some_and(|active| (active - length).abs() < 0.02)
                {
                    ops.push(PadOp::Engine(EngineCommand::LoopToggle { deck }));
                } else {
                    ops.push(PadOp::Engine(EngineCommand::AutoLoop {
                        deck,
                        length: Beats(length),
                    }));
                }
            }
        }
        PadMode::Sampler => {
            let Some(slot) = pads.sampler().slot_for_pad(deck, pad) else {
                return (ops, false);
            };
            if shifted {
                if pressed {
                    match (ctx.sampler_loaded[slot.index()], ctx.browse_track) {
                        (false, Some(track)) => ops.push(PadOp::SamplerLoad { slot, track }),
                        _ => ops.push(PadOp::SamplerStop(slot)),
                    }
                }
            } else {
                ops.push(PadOp::Sampler { slot, pressed });
            }
        }
        PadMode::Keyboard => match pads.keyboard_cue(deck) {
            None => {
                if pressed
                    && !shifted
                    && let Some(index) = cue
                {
                    if !ctx.hot_cue_set[index.index()] {
                        ops.push(PadOp::SetHotCue { deck, index });
                    }
                    pads.set_keyboard_cue(deck, Some(pad));
                }
            }
            Some(selected) => {
                let Some(index) = HotCueIndex::new(selected) else {
                    return (ops, false);
                };
                if shifted {
                    if pressed {
                        match pad {
                            0..=5 => {
                                ops.push(PadOp::DeleteHotCue { deck, index });
                                pads.set_keyboard_cue(deck, None);
                            }
                            6 => pads.adjust_keyboard(deck, 1),
                            _ => pads.adjust_keyboard(deck, -1),
                        }
                    }
                } else if pressed {
                    // The engine has no per-trigger pitch: the deck's key
                    // shift is set, then the cue plays. TODO(verify): add a
                    // pitched hot cue command if the engine gets one.
                    ops.push(PadOp::Engine(EngineCommand::SetKeyShift {
                        deck,
                        semitones: pad_semitones(pad, pads.keyboard_offset(deck)),
                    }));
                    started = true;
                    ops.push(PadOp::HotCue {
                        deck,
                        index,
                        pressed: true,
                    });
                } else if was_held {
                    ops.push(PadOp::HotCue {
                        deck,
                        index,
                        pressed: false,
                    });
                }
            }
        },
        PadMode::KeyShift => {
            if pressed {
                if shifted {
                    match pad {
                        6 => pads.adjust_key(deck, 1),
                        7 => pads.adjust_key(deck, -1),
                        _ => {}
                    }
                } else {
                    ops.push(PadOp::Engine(EngineCommand::SetKeyShift {
                        deck,
                        semitones: pad_semitones(pad, pads.key_offset(deck)),
                    }));
                }
            }
        }
    }
    (ops, started)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::SamplerSlotId;

    fn deck(n: u8) -> DeckId {
        DeckId::new(n).expect("deck")
    }

    fn run(
        pads: &PadModes,
        d: u8,
        mode: PadMode,
        pad: u8,
        shifted: bool,
        pressed: bool,
        ctx: &PadContext,
    ) -> Vec<PadOp> {
        if pads.get(deck(d)) != mode {
            pads.set(deck(d), mode);
        }
        plan(pads, deck(d), mode, pad, shifted, pressed, false, ctx).0
    }

    fn idx(i: u8) -> HotCueIndex {
        HotCueIndex::new(i).expect("cue")
    }

    #[test]
    fn hot_cue_sets_triggers_and_deletes() {
        let pads = PadModes::default();
        let mut ctx = PadContext::default();
        assert_eq!(
            run(&pads, 0, PadMode::HotCue, 2, false, true, &ctx),
            vec![PadOp::SetHotCue {
                deck: deck(0),
                index: idx(2)
            }]
        );
        ctx.hot_cue_set[2] = true;
        assert_eq!(
            run(&pads, 0, PadMode::HotCue, 2, false, true, &ctx),
            vec![PadOp::HotCue {
                deck: deck(0),
                index: idx(2),
                pressed: true
            }]
        );
        let (ops, started) = plan(&pads, deck(0), PadMode::HotCue, 2, false, true, false, &ctx);
        assert!(started && ops.len() == 1);
        let (ops, _) = plan(&pads, deck(0), PadMode::HotCue, 2, false, false, true, &ctx);
        assert_eq!(
            ops,
            vec![PadOp::HotCue {
                deck: deck(0),
                index: idx(2),
                pressed: false
            }]
        );
        assert_eq!(
            run(&pads, 0, PadMode::HotCue, 2, true, true, &ctx),
            vec![PadOp::DeleteHotCue {
                deck: deck(0),
                index: idx(2)
            }]
        );
    }

    #[test]
    fn pad_fx_banks_and_shift_layers() {
        let pads = PadModes::default();
        let ctx = PadContext::default();
        assert_eq!(
            run(&pads, 1, PadMode::PadFx2, 5, true, true, &ctx),
            vec![PadOp::FxPad {
                deck: deck(1),
                bank: 1,
                shift: true,
                pad: 5,
                pressed: true
            }]
        );
        assert_eq!(
            run(&pads, 0, PadMode::PadFx1, 0, false, false, &ctx),
            vec![PadOp::FxPad {
                deck: deck(0),
                bank: 0,
                shift: false,
                pad: 0,
                pressed: false
            }]
        );
    }

    #[test]
    fn beat_jump_sizes_follow_the_manual_and_shift_scales_them() {
        let pads = PadModes::default();
        let ctx = PadContext::default();
        let jump = |ops: Vec<PadOp>| match ops.as_slice() {
            [PadOp::Engine(EngineCommand::BeatJump { beats, .. })] => beats.0,
            other => panic!("unexpected {other:?}"),
        };
        let expected = [-1.0, 1.0, -2.0, 2.0, -4.0, 4.0, -8.0, 8.0];
        for (pad, want) in expected.iter().enumerate() {
            let got = jump(run(
                &pads,
                0,
                PadMode::BeatJump,
                pad as u8,
                false,
                true,
                &ctx,
            ));
            assert_eq!(got, *want, "pad {pad}");
        }
        assert!(run(&pads, 0, PadMode::BeatJump, 7, true, true, &ctx).is_empty());
        assert_eq!(pads.jump_step(deck(0)), 1);
        assert_eq!(
            jump(run(&pads, 0, PadMode::BeatJump, 0, false, true, &ctx)),
            -2.0
        );
        run(&pads, 0, PadMode::BeatJump, 6, true, true, &ctx);
        run(&pads, 0, PadMode::BeatJump, 6, true, true, &ctx);
        assert_eq!(
            jump(run(&pads, 0, PadMode::BeatJump, 0, false, true, &ctx)),
            -0.5
        );
        for _ in 0..5 {
            run(&pads, 0, PadMode::BeatJump, 6, true, true, &ctx);
        }
        assert_eq!(pads.jump_step(deck(0)), JUMP_STEP_RANGE.0);
        assert_eq!(pads.jump_step(deck(1)), 0, "per deck");
    }

    #[test]
    fn beat_loop_starts_and_exits() {
        let pads = PadModes::default();
        let mut ctx = PadContext::default();
        assert_eq!(
            run(&pads, 0, PadMode::BeatLoop, 4, false, true, &ctx),
            vec![PadOp::Engine(EngineCommand::AutoLoop {
                deck: deck(0),
                length: Beats(4.0)
            })]
        );
        ctx.loop_beats = Some(4.0);
        assert_eq!(
            run(&pads, 0, PadMode::BeatLoop, 4, false, true, &ctx),
            vec![PadOp::Engine(EngineCommand::LoopToggle { deck: deck(0) })]
        );
        assert!(matches!(
            run(&pads, 0, PadMode::BeatLoop, 5, false, true, &ctx).as_slice(),
            [PadOp::Engine(EngineCommand::AutoLoop { length, .. })] if length.0 == 8.0
        ));
        assert!(run(&pads, 0, PadMode::BeatLoop, 5, false, false, &ctx).is_empty());
    }

    #[test]
    fn sampler_pads_follow_the_layout() {
        let pads = PadModes::default();
        let ctx = PadContext::default();
        let slot = |n| SamplerSlotId::new(n).expect("slot");
        assert_eq!(
            run(&pads, 1, PadMode::Sampler, 2, false, true, &ctx),
            vec![PadOp::Sampler {
                slot: slot(10),
                pressed: true
            }]
        );
        assert_eq!(
            run(&pads, 0, PadMode::Sampler, 2, false, false, &ctx),
            vec![PadOp::Sampler {
                slot: slot(2),
                pressed: false
            }]
        );
        assert_eq!(
            run(&pads, 0, PadMode::Sampler, 2, true, true, &ctx),
            vec![PadOp::SamplerStop(slot(2))]
        );
        let track = TrackId::from(7);
        let loading = PadContext {
            browse_track: Some(track),
            ..PadContext::default()
        };
        assert_eq!(
            run(&pads, 0, PadMode::Sampler, 2, true, true, &loading),
            vec![PadOp::SamplerLoad {
                slot: slot(2),
                track
            }]
        );
        pads.sampler()
            .set_layout(crate::sampler::SamplerLayout::Shared);
        assert_eq!(
            run(&pads, 1, PadMode::Sampler, 2, false, true, &ctx),
            vec![PadOp::Sampler {
                slot: slot(2),
                pressed: true
            }]
        );
    }

    #[test]
    fn key_shift_pads_set_the_deck_key() {
        let pads = PadModes::default();
        let ctx = PadContext::default();
        let semis = |ops: Vec<PadOp>| match ops.as_slice() {
            [PadOp::Engine(EngineCommand::SetKeyShift { semitones, .. })] => *semitones,
            other => panic!("unexpected {other:?}"),
        };
        let want = [4, 5, 6, 7, 0, 1, 2, 3];
        for (pad, w) in want.iter().enumerate() {
            assert_eq!(
                semis(run(
                    &pads,
                    0,
                    PadMode::KeyShift,
                    pad as u8,
                    false,
                    true,
                    &ctx
                )),
                *w
            );
        }
        run(&pads, 0, PadMode::KeyShift, 6, true, true, &ctx);
        assert_eq!(
            semis(run(&pads, 0, PadMode::KeyShift, 4, false, true, &ctx)),
            1
        );
        run(&pads, 0, PadMode::KeyShift, 7, true, true, &ctx);
        run(&pads, 0, PadMode::KeyShift, 7, true, true, &ctx);
        assert_eq!(
            semis(run(&pads, 0, PadMode::KeyShift, 0, false, true, &ctx)),
            3
        );
        for _ in 0..30 {
            run(&pads, 0, PadMode::KeyShift, 6, true, true, &ctx);
        }
        assert_eq!(
            semis(run(&pads, 0, PadMode::KeyShift, 3, false, true, &ctx)),
            12
        );
    }

    #[test]
    fn keyboard_picks_a_cue_then_plays_it_pitched() {
        let pads = PadModes::default();
        let mut ctx = PadContext::default();
        // No cue chosen: an empty slot is set, and chosen.
        assert_eq!(
            run(&pads, 0, PadMode::Keyboard, 1, false, true, &ctx),
            vec![PadOp::SetHotCue {
                deck: deck(0),
                index: idx(1)
            }]
        );
        assert_eq!(pads.keyboard_cue(deck(0)), Some(1));
        // Chosen: pad 6 is +1 semitone.
        let (ops, started) = plan(
            &pads,
            deck(0),
            PadMode::Keyboard,
            5,
            false,
            true,
            false,
            &ctx,
        );
        assert!(started);
        assert_eq!(
            ops,
            vec![
                PadOp::Engine(EngineCommand::SetKeyShift {
                    deck: deck(0),
                    semitones: 1
                }),
                PadOp::HotCue {
                    deck: deck(0),
                    index: idx(1),
                    pressed: true
                },
            ]
        );
        let (ops, _) = plan(
            &pads,
            deck(0),
            PadMode::Keyboard,
            5,
            false,
            false,
            true,
            &ctx,
        );
        assert_eq!(
            ops,
            vec![PadOp::HotCue {
                deck: deck(0),
                index: idx(1),
                pressed: false
            }]
        );
        // SHIFT + pad 7 raises the layout, SHIFT + pad 3 deletes the cue and forgets it.
        run(&pads, 0, PadMode::Keyboard, 6, true, true, &ctx);
        assert_eq!(pads.keyboard_offset(deck(0)), 1);
        assert_eq!(
            run(&pads, 0, PadMode::Keyboard, 2, true, true, &ctx),
            vec![PadOp::DeleteHotCue {
                deck: deck(0),
                index: idx(1)
            }]
        );
        assert_eq!(pads.keyboard_cue(deck(0)), None);
        // A set cue is chosen without being overwritten; choosing KEYBOARD again forgets it.
        ctx.hot_cue_set[3] = true;
        assert!(run(&pads, 0, PadMode::Keyboard, 3, false, true, &ctx).is_empty());
        assert_eq!(pads.keyboard_cue(deck(0)), Some(3));
        pads.set(deck(0), PadMode::Keyboard);
        assert_eq!(pads.keyboard_cue(deck(0)), None);
    }

    #[test]
    fn mode_changes_are_shared_between_clones() {
        let a = PadModes::default();
        let b = a.clone();
        assert!(a.set(deck(0), PadMode::Sampler));
        assert_eq!(b.get(deck(0)), PadMode::Sampler);
        assert!(!b.set(deck(0), PadMode::Sampler));
        assert_eq!(b.get(deck(1)), PadMode::HotCue);
        assert_eq!(b.state().decks[0].mode, PadMode::Sampler);
    }

    #[test]
    fn select_announces_only_real_changes() {
        let pads = PadModes::default();
        let rx = pads.subscribe();
        assert!(pads.select(deck(0), PadMode::KeyShift));
        assert!(!pads.select(deck(0), PadMode::KeyShift));
        assert_eq!(
            rx.try_recv().expect("event"),
            PadEvent::Mode(PadModeChanged {
                deck: deck(0),
                mode: PadMode::KeyShift
            })
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn state_events_reach_subscribers() {
        let pads = PadModes::default();
        let rx = pads.subscribe();
        pads.adjust_jump(deck(1), 1);
        match rx.try_recv().expect("event") {
            PadEvent::State(s) => assert_eq!(s.decks[1].jump_beats[0], -2.0),
            other => panic!("unexpected {other:?}"),
        }
    }
}
