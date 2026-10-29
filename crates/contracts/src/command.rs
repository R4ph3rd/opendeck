//! Commands the control thread sends to the audio thread.
//!
//! [`EngineCommand`] is everything the audio callback can be told. It is
//! `Copy`, owns no heap memory and fits in a few machine words, so it can be
//! pushed through an `rtrb` SPSC ring without allocation (ADR 0003).
//!
//! Heap-owning hand-offs (decoded track buffers, prepared effect instances,
//! sampler buffers) do not go through this enum. They travel on dedicated
//! rings owned by the engine, and the audio thread returns replaced objects on
//! a return ring so they are freed off the audio thread.
//!
//! Positions are [`Frames`] at the engine sample rate. Values are applied at
//! the start of the next audio block; continuous parameters are smoothed by
//! the engine to avoid zipper noise. Out-of-range values are clamped by the
//! engine, never rejected with a panic.

use crate::cue::{FrameRegion, HotCueIndex};
use crate::fx::ParamId;
use crate::ids::{DeckId, FxSlotId, SamplerSlotId};
use crate::mixer::{CrossfaderAssign, CrossfaderCurve, EqBand};
use crate::tempo::{SyncMode, TempoRange};
use crate::units::{Beats, Frames, Gain};

/// A command for the audio thread. RT-safe: `Copy`, no heap, no drop glue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineCommand {
    // ----- Transport -----
    /// Start or stop playback.
    SetPlaying {
        /// Target deck.
        deck: DeckId,
        /// `true` to play.
        playing: bool,
    },
    /// CUE button edge (CDJ behaviour): pressed while playing returns to the
    /// cue point and pauses; pressed while paused off the cue point sets it;
    /// held while paused on the cue point previews until release.
    Cue {
        /// Target deck.
        deck: DeckId,
        /// `true` on press, `false` on release.
        pressed: bool,
    },
    /// Set the main cue point explicitly (memory cue recall, UI).
    SetCuePoint {
        /// Target deck.
        deck: DeckId,
        /// New cue point.
        position: Frames,
    },
    /// Move the playhead. Quantize does not apply.
    Seek {
        /// Target deck.
        deck: DeckId,
        /// New playhead position.
        position: Frames,
    },
    /// Unload the track; the buffer goes back on the return ring.
    Eject {
        /// Target deck.
        deck: DeckId,
    },

    // ----- Tempo, key, sync -----
    /// Tempo fader position in `[-1, 1]`, `+1` = fastest.
    SetTempoFader {
        /// Target deck.
        deck: DeckId,
        /// Fader position.
        position: f32,
    },
    /// Tempo fader range.
    SetTempoRange {
        /// Target deck.
        deck: DeckId,
        /// New range.
        range: TempoRange,
    },
    /// Key lock (master tempo) on or off.
    SetKeyLock {
        /// Target deck.
        deck: DeckId,
        /// `true` to keep pitch when tempo changes.
        on: bool,
    },
    /// Key shift in semitones, `-12..=12`.
    SetKeyShift {
        /// Target deck.
        deck: DeckId,
        /// Semitones, positive = up.
        semitones: i8,
    },
    /// Sync mode for a deck.
    SetSync {
        /// Target deck.
        deck: DeckId,
        /// New mode.
        mode: SyncMode,
    },
    /// Choose the sync leader; `None` lets the engine pick the first playing synced deck.
    SetLeader {
        /// New leader.
        deck: Option<DeckId>,
    },
    /// Temporary speed offset while held (pitch bend buttons, jog wheel side).
    /// `amount` in `[-1, 1]` maps to the engine's maximum bend; `0` releases.
    PitchBend {
        /// Target deck.
        deck: DeckId,
        /// Bend amount.
        amount: f32,
    },

    // ----- Jog -----
    /// Platter touched or released. In vinyl mode touch holds the record.
    JogTouch {
        /// Target deck.
        deck: DeckId,
        /// `true` while touched.
        touched: bool,
    },
    /// Platter rotation since the last message, in revolutions (signed,
    /// positive = clockwise). Scratches when touched in vinyl mode, nudges
    /// tempo otherwise. One revolution is `VINYL_SECONDS_PER_REVOLUTION` of audio.
    JogTurn {
        /// Target deck.
        deck: DeckId,
        /// Revolutions since the previous event.
        revolutions: f32,
    },
    /// Fast search (SHIFT + jog), in beats to move (signed).
    JogSearch {
        /// Target deck.
        deck: DeckId,
        /// Beats to move.
        beats: Beats,
    },
    /// Vinyl mode on or off.
    SetVinylMode {
        /// Target deck.
        deck: DeckId,
        /// `true` for vinyl (scratch) mode.
        on: bool,
    },

    // ----- Loops, cues, jumps -----
    /// Set loop in at the playhead (quantized if quantize is on).
    LoopIn {
        /// Target deck.
        deck: DeckId,
    },
    /// Set loop out at the playhead and start looping.
    LoopOut {
        /// Target deck.
        deck: DeckId,
    },
    /// Exit the active loop, or re-enter the last loop if none is active.
    LoopToggle {
        /// Target deck.
        deck: DeckId,
    },
    /// Start an auto beat loop of `length` from the playhead (quantized).
    AutoLoop {
        /// Target deck.
        deck: DeckId,
        /// Loop length, positive.
        length: Beats,
    },
    /// Loop roll: loop while held, then resume where playback would have been.
    LoopRoll {
        /// Target deck.
        deck: DeckId,
        /// Roll length, positive.
        length: Beats,
        /// `true` on press, `false` on release.
        active: bool,
    },
    /// Multiply the active loop length (`0.5` halves, `2.0` doubles).
    LoopResize {
        /// Target deck.
        deck: DeckId,
        /// Factor, positive.
        factor: f32,
    },
    /// Set and activate an explicit loop (saved loop recall). `region.end` must be `Some`.
    SetLoop {
        /// Target deck.
        deck: DeckId,
        /// Loop region in engine frames.
        region: FrameRegion,
    },
    /// Jump by `beats` (signed). Keeps phase; moves the loop with the playhead when looping.
    BeatJump {
        /// Target deck.
        deck: DeckId,
        /// Beats to jump, negative = backwards.
        beats: Beats,
    },
    /// Define or clear a hot cue slot. Sent on track load and on edits.
    SetHotCue {
        /// Target deck.
        deck: DeckId,
        /// Pad slot.
        index: HotCueIndex,
        /// Cue point or hot loop, `None` to clear.
        cue: Option<FrameRegion>,
    },
    /// Hot cue pad edge. On an empty slot the engine stores the playhead and
    /// reports `EngineEvent::HotCueSet`; otherwise it jumps (or previews
    /// while held when paused).
    HotCue {
        /// Target deck.
        deck: DeckId,
        /// Pad slot.
        index: HotCueIndex,
        /// `true` on press, `false` on release.
        pressed: bool,
    },
    /// Slip mode on or off.
    SetSlip {
        /// Target deck.
        deck: DeckId,
        /// `true` to enable.
        on: bool,
    },
    /// Quantize on or off.
    SetQuantize {
        /// Target deck.
        deck: DeckId,
        /// `true` to enable.
        on: bool,
    },

    // ----- Mixer -----
    /// Channel input trim.
    SetTrim {
        /// Target channel.
        deck: DeckId,
        /// Linear gain.
        gain: Gain,
    },
    /// Channel EQ band gain (`0` = kill).
    SetEq {
        /// Target channel.
        deck: DeckId,
        /// Band.
        band: EqBand,
        /// Linear gain.
        gain: Gain,
    },
    /// Channel filter in `[-1, 1]`: negative low-pass, positive high-pass, 0 off.
    SetFilter {
        /// Target channel.
        deck: DeckId,
        /// Filter amount.
        amount: f32,
    },
    /// Channel fader position in `[0, 1]`.
    SetChannelFader {
        /// Target channel.
        deck: DeckId,
        /// Fader position.
        position: f32,
    },
    /// Headphone cue (PFL) on or off.
    SetPfl {
        /// Target channel.
        deck: DeckId,
        /// `true` to send the channel to the cue bus.
        on: bool,
    },
    /// Crossfader side for a channel.
    SetCrossfaderAssign {
        /// Target channel.
        deck: DeckId,
        /// Side.
        assign: CrossfaderAssign,
    },
    /// Crossfader position in `[-1, 1]`, `-1` = full A.
    SetCrossfader {
        /// Position.
        position: f32,
    },
    /// Crossfader curve.
    SetCrossfaderCurve {
        /// Curve.
        curve: CrossfaderCurve,
    },
    /// Master output level.
    SetMasterLevel {
        /// Linear gain.
        gain: Gain,
    },
    /// Headphone output level.
    SetHeadphoneLevel {
        /// Linear gain.
        gain: Gain,
    },
    /// Headphone cue/master mix in `[0, 1]`, 0 = cue only.
    SetHeadphoneMix {
        /// Mix amount.
        mix: f32,
    },
    /// Split cue on or off.
    SetSplitCue {
        /// `true` for cue left, master right.
        on: bool,
    },

    // ----- Effects -----
    /// Enable or bypass an effect slot (click-free crossfade, tail kept).
    SetFxEnabled {
        /// Slot.
        slot: FxSlotId,
        /// `true` to enable.
        on: bool,
    },
    /// Set an effect parameter, normalized `[0, 1]` (see `ParamDescriptor`).
    SetFxParam {
        /// Slot.
        slot: FxSlotId,
        /// Parameter id from the effect's descriptor.
        param: ParamId,
        /// Normalized value.
        value: f32,
    },
    /// Wet/dry mix for a slot in `[0, 1]`, applied by the host.
    SetFxMix {
        /// Slot.
        slot: FxSlotId,
        /// 0 = dry, 1 = wet.
        wet: f32,
    },
    /// Beat-synced time division for a slot (e.g. `0.5` = half a beat).
    SetFxBeatDivision {
        /// Slot.
        slot: FxSlotId,
        /// Division, positive.
        beats: Beats,
    },
    /// Level a channel sends to a send/return bus.
    SetFxSend {
        /// Source channel.
        deck: DeckId,
        /// Bus index `0..FX_SEND_BUSES`.
        bus: u8,
        /// Linear send level.
        level: Gain,
    },

    // ----- Sampler -----
    /// Sampler pad edge.
    SamplerTrigger {
        /// Slot.
        slot: SamplerSlotId,
        /// `true` on press, `false` on release.
        pressed: bool,
    },
    /// Stop a sampler slot.
    SamplerStop {
        /// Slot.
        slot: SamplerSlotId,
    },
    /// Sampler bus level.
    SetSamplerLevel {
        /// Linear gain.
        gain: Gain,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_copy<T: Copy + Send + 'static>() {}

    #[test]
    fn command_is_copy_and_small() {
        assert_copy::<EngineCommand>();
        assert!(!core::mem::needs_drop::<EngineCommand>());
        assert!(
            core::mem::size_of::<EngineCommand>() <= 40,
            "EngineCommand grew to {} bytes",
            core::mem::size_of::<EngineCommand>()
        );
    }
}
