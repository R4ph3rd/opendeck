//! Mixer types: crossfader curves, channel strip and mixer snapshots.

use crate::MAX_DECKS;
use crate::ids::DeckId;
use crate::units::Gain;

/// Crossfader curve. Gains are computed by [`CrossfaderCurve::gains`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CrossfaderCurve {
    /// Constant power: `a = cos(x * pi/2)`, `b = sin(x * pi/2)`. Both sides at
    /// -3 dB in the centre, no perceived volume dip. Default for blending.
    #[default]
    ConstantPower,
    /// Linear: `a = 1 - x`, `b = x`. Both sides at -6 dB in the centre (dip).
    Linear,
    /// Scratch cut: each side stays at unity and cuts over the last
    /// [`SHARP_CUT_WIDTH`] of travel at the opposite edge.
    Sharp,
}

/// Fraction of crossfader travel used for the cut in [`CrossfaderCurve::Sharp`].
pub const SHARP_CUT_WIDTH: f32 = 0.05;

impl CrossfaderCurve {
    /// Gains for the A and B sides at `position` in `[-1, 1]` (`-1` = full A).
    /// Out-of-range positions are clamped. RT-safe.
    pub fn gains(self, position: f32) -> (Gain, Gain) {
        let x = (position.clamp(-1.0, 1.0) + 1.0) * 0.5;
        let (a, b) = match self {
            CrossfaderCurve::ConstantPower => {
                let angle = x * core::f32::consts::FRAC_PI_2;
                (angle.cos(), angle.sin())
            }
            CrossfaderCurve::Linear => (1.0 - x, x),
            CrossfaderCurve::Sharp => (
                ((1.0 - x) / SHARP_CUT_WIDTH).min(1.0),
                (x / SHARP_CUT_WIDTH).min(1.0),
            ),
        };
        (Gain(a), Gain(b))
    }
}

/// Which crossfader side a channel is assigned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum CrossfaderAssign {
    /// Left side (A).
    A,
    /// Bypasses the crossfader.
    #[default]
    Thru,
    /// Right side (B).
    B,
}

/// One band of the 3-band channel EQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum EqBand {
    /// Low band.
    Low,
    /// Mid band.
    Mid,
    /// High band.
    High,
}

impl EqBand {
    /// Array index: Low = 0, Mid = 1, High = 2.
    pub const fn index(self) -> usize {
        match self {
            EqBand::Low => 0,
            EqBand::Mid => 1,
            EqBand::High => 2,
        }
    }
}

/// Maximum EQ boost. EQ gain range is `0.0` (full kill) to `EQ_MAX_GAIN_DB`.
/// Product decision; the knob curve mapping lives in app-core.
pub const EQ_MAX_GAIN_DB: f32 = 6.0;

/// Maximum trim boost. Trim range is `-inf` to `TRIM_MAX_GAIN_DB`.
pub const TRIM_MAX_GAIN_DB: f32 = 12.0;

/// Snapshot of one channel strip, for the UI and session restore. Not sent
/// to the audio thread as a whole: every field has its own `EngineCommand`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ChannelState {
    /// Deck feeding this channel.
    pub deck: DeckId,
    /// Input trim, linear.
    pub trim: Gain,
    /// EQ gains indexed by [`EqBand::index`], linear, `0..=db_to_gain(EQ_MAX_GAIN_DB)`.
    pub eq: [Gain; 3],
    /// Filter amount in `[-1, 1]`: negative = low-pass, positive = high-pass, 0 = off.
    pub filter: f32,
    /// Channel fader position in `[0, 1]`; the engine applies the fader curve.
    pub fader: f32,
    /// Headphone cue (PFL) on.
    pub pfl: bool,
    /// Crossfader side.
    pub crossfader_assign: CrossfaderAssign,
}

impl ChannelState {
    /// Neutral strip: unity trim and EQ, filter off, fader up, PFL off.
    pub fn neutral(deck: DeckId) -> Self {
        Self {
            deck,
            trim: Gain::UNITY,
            eq: [Gain::UNITY; 3],
            filter: 0.0,
            fader: 1.0,
            pfl: false,
            crossfader_assign: CrossfaderAssign::Thru,
        }
    }
}

/// Snapshot of the whole mixer, for the UI and session restore.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MixerState {
    /// One strip per deck, indexed by [`DeckId::index`].
    pub channels: [ChannelState; MAX_DECKS],
    /// Crossfader position in `[-1, 1]`.
    pub crossfader: f32,
    /// Crossfader curve.
    pub crossfader_curve: CrossfaderCurve,
    /// Master output level, linear.
    pub master_level: Gain,
    /// Headphone output level, linear.
    pub headphone_level: Gain,
    /// Headphone mix in `[0, 1]`: 0 = cue only, 1 = master only.
    pub headphone_mix: f32,
    /// Split cue: cue bus summed to mono on the left ear, master mono on the right.
    pub split_cue: bool,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            channels: DeckId::ALL.map(ChannelState::neutral),
            crossfader: 0.0,
            crossfader_curve: CrossfaderCurve::default(),
            master_level: Gain::UNITY,
            headphone_level: Gain::UNITY,
            headphone_mix: 0.0,
            split_cue: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn constant_power_curve() {
        let (a, b) = CrossfaderCurve::ConstantPower.gains(0.0);
        assert!(close(a.0, core::f32::consts::FRAC_1_SQRT_2));
        assert!(close(b.0, core::f32::consts::FRAC_1_SQRT_2));
        let (a, b) = CrossfaderCurve::ConstantPower.gains(-1.0);
        assert!(close(a.0, 1.0) && close(b.0, 0.0));
    }

    #[test]
    fn linear_curve_dips() {
        let (a, b) = CrossfaderCurve::Linear.gains(0.0);
        assert!(close(a.0, 0.5) && close(b.0, 0.5));
    }

    #[test]
    fn sharp_curve_cuts_at_edges() {
        let (a, b) = CrossfaderCurve::Sharp.gains(0.0);
        assert!(close(a.0, 1.0) && close(b.0, 1.0));
        let (a, b) = CrossfaderCurve::Sharp.gains(1.0);
        assert!(close(a.0, 0.0) && close(b.0, 1.0));
        let (a, _) = CrossfaderCurve::Sharp.gains(0.95);
        assert!(close(a.0, 0.5));
    }

    #[test]
    fn default_mixer_is_neutral() {
        let m = MixerState::default();
        assert_eq!(m.channels[1].deck.index(), 1);
        assert_eq!(m.crossfader_curve, CrossfaderCurve::ConstantPower);
    }
}
