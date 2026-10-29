//! The deck side of sync. Election and rate maths live in `crate::sync`.

use opendeck_contracts::beatgrid::BeatGrid;
use opendeck_contracts::tempo::SyncMode;

use super::{Deck, Transport};

/// What sync is doing for a deck, as reported by [`DeckStatus`](super::DeckStatus).
/// Telemetry carries the mode and the leader; this finer state (no grid, no
/// leader) is engine-local for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStatus {
    /// Sync is off.
    #[default]
    Off,
    /// Sync is on but the deck has no beat grid or no track: nothing happens.
    NoGrid,
    /// Sync is on but there is no leader to follow: the deck holds its tempo.
    NoLeader,
    /// This deck is the leader.
    Leader,
    /// Following the leader (tempo, and phase when the mode is beat sync).
    Following,
}

/// Largest rate change the deck takes without ramping. Bigger steps (sync
/// engaging, a leader change) apply at once so phase is not smeared over a block.
const RATE_SNAP: f64 = 0.002;

impl Deck {
    pub(crate) fn grid(&self) -> Option<BeatGrid> {
        self.grid.filter(|_| self.track.is_some())
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.track.is_some() && self.transport != Transport::Stopped
    }

    pub(crate) fn position(&self) -> f64 {
        self.pos
    }

    pub(crate) fn sync_mode(&self) -> SyncMode {
        self.sync_mode
    }

    pub(crate) fn set_sync_status(&mut self, status: SyncStatus) {
        self.sync_status = status;
    }

    pub(crate) fn set_sync_mode(&mut self, mode: SyncMode) {
        self.sync_mode = mode;
        if mode == SyncMode::Off {
            self.held_rate = None;
            self.phase_trim = 0.0;
            self.needs_align = false;
        }
        self.mark_align();
    }

    /// Asks for a phase alignment at the next sync update.
    pub(super) fn mark_align(&mut self) {
        if self.sync_mode == SyncMode::Beat {
            self.needs_align = true;
        }
    }

    /// Returns and clears the alignment request.
    pub(crate) fn take_align(&mut self) -> bool {
        std::mem::take(&mut self.needs_align)
    }

    /// Follows the leader: tempo `rate` (ratio to original speed) plus a small
    /// phase correction `trim`.
    pub(crate) fn follow(&mut self, rate: f64, trim: f64) {
        self.held_rate = Some(rate.clamp(0.0, super::MAX_RATE));
        self.phase_trim = trim.clamp(-0.01, 0.01);
        let target = self.target_rate();
        if self.scratch.is_none() && (target - self.rate_cur).abs() > RATE_SNAP {
            self.rate_cur = target;
        }
    }

    /// Stops phase correction; the tempo hold stays.
    pub(crate) fn hold(&mut self) {
        self.phase_trim = 0.0;
    }

    /// Moves the playhead without a transition (a silent phase alignment
    /// right after a jump, or while stopped).
    pub(crate) fn shift(&mut self, delta: f64) {
        if self.track.is_none() || !delta.is_finite() {
            return;
        }
        self.pos = (self.pos + delta).clamp(0.0, self.len_frames());
        if self.key_lock && self.audible() && self.scratch.is_none() {
            self.begin_priming();
        }
    }
}
