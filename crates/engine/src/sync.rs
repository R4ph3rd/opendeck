//! Sync: leader election and follower tempo and phase.
//!
//! Runs once per block on the audio thread, after the commands, before the
//! decks render. It reads each deck's grid, playhead and tempo and tells the
//! followers what rate to run at. Nothing is allocated.
//!
//! # Rules
//!
//! - A deck follows when its sync mode is not `Off`, it has a track with a
//!   beat grid, and there is a leader other than itself. Without a grid sync
//!   does nothing and the deck reports `SyncStatus::NoGrid`.
//! - The leader is the deck chosen with `SetLeader` while it is playing with a
//!   grid. Otherwise the previous leader stays while it is valid, else the
//!   first playing deck with sync off (the deck being followed is the one
//!   already playing, so pressing sync on the second deck follows the first),
//!   else the first playing deck. Only decks with a grid qualify. When the leader stops or is ejected the next block
//!   elects another, so followers never wait on a dead deck. With no sync deck
//!   and no explicit choice there is no leader.
//! - Tempo: a follower runs at `leader_bpm * leader_rate / own_bpm`, times 0.5
//!   or 2 when that is closer to its own tempo (half and double time).
//!   The leader's rate includes its pitch bend and nudge, so followers stay
//!   with it. The follower's tempo fader is ignored while it follows. When it
//!   stops following (sync off) it returns to the fader; when the leader is
//!   lost it holds its last synced tempo until the fader moves.
//! - Beat sync adds phase: the follower's beat phase is made equal to the
//!   leader's (times the half or double factor). It aligns by one jump when
//!   sync engages, when the deck starts playing and after jumps (hot cue,
//!   loop, slip return). Between those only a gentle correction runs
//!   ([`TRIM_WINDOW`]), so a deliberate nudge or scratch is not undone; press
//!   beat sync again to re-align.

use opendeck_contracts::tempo::SyncMode;
use opendeck_contracts::{DeckId, Seconds};

use crate::deck::{Deck, SyncStatus};

/// Phase errors under this (seconds) are corrected by a small rate trim.
const TRIM_WINDOW: f64 = 0.010;

/// Time constant of the phase trim, seconds.
const TRIM_TAU: f64 = 0.25;

/// Leader and election state.
pub(crate) struct SyncState {
    sample_rate: f64,
    preferred: Option<usize>,
    leader: Option<usize>,
}

/// Tempo and phase of the leader at the start of a block.
#[derive(Clone, Copy)]
struct LeaderView {
    /// Effective BPM (grid BPM times rate).
    bpm: f64,
    /// Fractional beat index at the playhead.
    beat: f64,
}

impl SyncState {
    pub(crate) fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            preferred: None,
            leader: None,
        }
    }

    /// The deck chosen with `SetLeader`, or `None` to let the engine pick.
    pub(crate) fn set_preferred(&mut self, deck: Option<DeckId>) {
        self.preferred = deck.map(DeckId::index);
    }

    pub(crate) fn leader(&self) -> Option<DeckId> {
        self.leader
            .and_then(|i| u8::try_from(i).ok())
            .and_then(DeckId::new)
    }

    pub(crate) fn update(&mut self, decks: &mut [Deck]) {
        let valid = |d: &Deck| d.is_playing() && d.grid().is_some();
        let wanted = decks
            .iter()
            .any(|d| d.sync_mode() != SyncMode::Off && d.grid().is_some());
        let explicit = self.preferred.filter(|i| decks.get(*i).is_some_and(valid));
        self.leader = explicit.or_else(|| {
            if !wanted {
                return None;
            }
            self.leader
                .filter(|i| decks.get(*i).is_some_and(valid))
                .or_else(|| {
                    decks
                        .iter()
                        .position(|d| d.sync_mode() == SyncMode::Off && valid(d))
                })
                .or_else(|| decks.iter().position(valid))
        });

        let view = self.leader.and_then(|i| {
            let d = decks.get(i)?;
            let grid = d.grid()?;
            Some(LeaderView {
                bpm: grid.bpm().0 * d.tempo_rate(),
                beat: grid.beat_at(Seconds(d.position() / self.sample_rate)).0,
            })
        });

        for (i, deck) in decks.iter_mut().enumerate() {
            let mode = deck.sync_mode();
            if mode == SyncMode::Off {
                deck.set_sync_status(SyncStatus::Off);
                continue;
            }
            let Some(grid) = deck.grid() else {
                deck.set_sync_status(SyncStatus::NoGrid);
                deck.hold();
                continue;
            };
            if Some(i) == self.leader {
                deck.set_sync_status(SyncStatus::Leader);
                deck.hold();
                continue;
            }
            let Some(lead) = view else {
                deck.set_sync_status(SyncStatus::NoLeader);
                deck.hold();
                continue;
            };
            deck.set_sync_status(SyncStatus::Following);

            let own = grid.bpm().0;
            let ratio = lead.bpm / own;
            let multiple = [0.5, 1.0, 2.0]
                .into_iter()
                .min_by(|a, b| (ratio * a).ln().abs().total_cmp(&(ratio * b).ln().abs()))
                .unwrap_or(1.0);
            let rate = ratio * multiple;
            let mut trim = 0.0;
            if mode == SyncMode::Beat && deck.is_playing() {
                let beat = grid.beat_at(Seconds(deck.position() / self.sample_rate)).0;
                let error =
                    wrap_half(beat.rem_euclid(1.0) - (multiple * lead.beat).rem_euclid(1.0));
                if deck.take_align() {
                    deck.shift(-error * 60.0 / own * self.sample_rate);
                } else {
                    let seconds = error * 60.0 / (own * rate.max(1e-3));
                    if seconds.abs() < TRIM_WINDOW {
                        trim = -seconds / TRIM_TAU;
                    }
                }
            }
            deck.follow(rate, trim);
        }
    }
}

/// Wraps a difference of two phases in `[0, 1)` to `[-0.5, 0.5)`.
fn wrap_half(x: f64) -> f64 {
    (x + 0.5).rem_euclid(1.0) - 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_difference_wraps_to_the_nearest_side() {
        assert!((wrap_half(0.9) + 0.1).abs() < 1e-12);
        assert!((wrap_half(-0.9) - 0.1).abs() < 1e-12);
        assert!(wrap_half(0.25) - 0.25 < 1e-12);
    }
}
