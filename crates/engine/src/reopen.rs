//! Reopening an audio device after it was lost.
//!
//! [`Reopener`] is a small state machine with time passed in, so tests drive
//! it without sleeping. After [`device_lost`](Reopener::device_lost) it asks
//! for an attempt after 250 ms, then 500 ms, 1 s, 2 s, 4 s and every 5 s
//! from then on, until [`opened`](Reopener::opened) or
//! [`cancel`](Reopener::cancel). Each attempt first checks that a device with
//! the configured name is present again (by name, or any default device when
//! no name is set), so a missing device costs one device listing, not a
//! failed stream build.
//!
//! The caller owns what a restart means (app-core rebuilds the engine, reloads
//! tracks and replays deck state); this type only decides when to try.

use std::time::{Duration, Instant};

use crate::device::AudioError;

/// First wait after a loss.
pub const REOPEN_INITIAL: Duration = Duration::from_millis(250);
/// Longest wait between attempts.
pub const REOPEN_MAX: Duration = Duration::from_secs(5);

/// Where the reopen machine is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReopenState {
    /// Nothing to do: the device is open or was never lost.
    Idle,
    /// The device is gone; the next attempt is at `next_try`.
    Waiting {
        /// When the device was lost.
        lost_at: Instant,
        /// Failed attempts so far.
        attempts: u32,
        /// When to try next.
        next_try: Instant,
    },
}

/// What one [`Reopener::poll`] call did.
#[derive(Debug, PartialEq, Eq)]
pub enum ReopenOutcome<S> {
    /// Nothing is lost, or it is not time yet.
    NotDue,
    /// The device is still absent; the machine backs off.
    Absent,
    /// The device is present but opening failed; the machine backs off.
    Failed(AudioError),
    /// The device is open again.
    Opened(S),
}

/// Backoff state machine for device reopening.
#[derive(Debug, Clone, Copy)]
pub struct Reopener {
    state: ReopenState,
}

impl Default for Reopener {
    fn default() -> Self {
        Self::new()
    }
}

impl Reopener {
    /// A machine in [`ReopenState::Idle`].
    pub const fn new() -> Self {
        Self {
            state: ReopenState::Idle,
        }
    }

    /// Current state.
    pub fn state(&self) -> ReopenState {
        self.state
    }

    /// The device is gone and attempts should start.
    pub fn is_waiting(&self) -> bool {
        matches!(self.state, ReopenState::Waiting { .. })
    }

    /// Failed attempts since the loss, 0 while idle.
    pub fn attempts(&self) -> u32 {
        match self.state {
            ReopenState::Waiting { attempts, .. } => attempts,
            ReopenState::Idle => 0,
        }
    }

    /// Records a loss at `now`. A loss while already waiting keeps the
    /// original schedule.
    pub fn device_lost(&mut self, now: Instant) {
        if !self.is_waiting() {
            self.state = ReopenState::Waiting {
                lost_at: now,
                attempts: 0,
                next_try: now + REOPEN_INITIAL,
            };
        }
    }

    /// An attempt should be made at `now`.
    pub fn due(&self, now: Instant) -> bool {
        matches!(self.state, ReopenState::Waiting { next_try, .. } if now >= next_try)
    }

    /// When the next attempt is due, `None` while idle.
    pub fn next_try(&self) -> Option<Instant> {
        match self.state {
            ReopenState::Waiting { next_try, .. } => Some(next_try),
            ReopenState::Idle => None,
        }
    }

    /// An attempt at `now` did not bring the device back; wait longer.
    pub fn failed(&mut self, now: Instant) {
        if let ReopenState::Waiting {
            lost_at, attempts, ..
        } = self.state
        {
            let attempts = attempts.saturating_add(1);
            self.state = ReopenState::Waiting {
                lost_at,
                attempts,
                next_try: now + backoff(attempts),
            };
        }
    }

    /// The device is open again.
    pub fn opened(&mut self) {
        self.state = ReopenState::Idle;
    }

    /// Gives up (the user picked another device, or shut down).
    pub fn cancel(&mut self) {
        self.state = ReopenState::Idle;
    }

    /// Runs one step: when an attempt is due, checks `present` and, if the
    /// device is there, calls `open`. Updates the machine from the result.
    pub fn poll<S>(
        &mut self,
        now: Instant,
        present: impl FnOnce() -> bool,
        open: impl FnOnce() -> Result<S, AudioError>,
    ) -> ReopenOutcome<S> {
        if !self.due(now) {
            return ReopenOutcome::NotDue;
        }
        if !present() {
            self.failed(now);
            return ReopenOutcome::Absent;
        }
        match open() {
            Ok(session) => {
                self.opened();
                ReopenOutcome::Opened(session)
            }
            Err(error) => {
                self.failed(now);
                ReopenOutcome::Failed(error)
            }
        }
    }
}

/// Wait after `attempts` failed attempts: 500 ms, 1 s, 2 s, 4 s, then 5 s.
fn backoff(attempts: u32) -> Duration {
    let factor = 1u32.checked_shl(attempts).unwrap_or(u32::MAX);
    REOPEN_INITIAL
        .checked_mul(factor)
        .map_or(REOPEN_MAX, |d| d.min(REOPEN_MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device that can be unplugged and replugged, and refuse to open.
    struct MockBackend {
        present: bool,
        busy: bool,
        opens: u32,
        checks: u32,
    }

    impl MockBackend {
        fn poll(&mut self, machine: &mut Reopener, now: Instant) -> ReopenOutcome<u32> {
            let (present, busy) = (self.present, self.busy);
            let (checks, opens) = (&mut self.checks, &mut self.opens);
            machine.poll(
                now,
                || {
                    *checks += 1;
                    present
                },
                || {
                    *opens += 1;
                    if busy {
                        Err(AudioError::Build("device busy".into()))
                    } else {
                        Ok(*opens)
                    }
                },
            )
        }
    }

    fn mock() -> MockBackend {
        MockBackend {
            present: true,
            busy: false,
            opens: 0,
            checks: 0,
        }
    }

    #[test]
    fn idle_machine_never_tries() {
        let mut machine = Reopener::new();
        let mut backend = mock();
        let t0 = Instant::now();
        assert_eq!(backend.poll(&mut machine, t0), ReopenOutcome::NotDue);
        assert!(!machine.due(t0 + Duration::from_secs(60)));
        assert_eq!((backend.checks, backend.opens), (0, 0));
    }

    #[test]
    fn unplug_then_replug_reopens_with_backoff() {
        let t0 = Instant::now();
        let mut machine = Reopener::new();
        let mut backend = mock();
        backend.present = false;
        machine.device_lost(t0);
        assert!(machine.is_waiting());

        // Not due before the first delay.
        assert_eq!(
            backend.poll(&mut machine, t0 + Duration::from_millis(100)),
            ReopenOutcome::NotDue
        );
        assert_eq!(backend.checks, 0);

        // Each attempt finds the device absent and waits longer.
        let mut now = t0;
        let mut waits = Vec::new();
        for _ in 0..7 {
            let due = machine.next_try().expect("waiting");
            waits.push(due - now);
            now = due;
            assert_eq!(backend.poll(&mut machine, now), ReopenOutcome::Absent);
        }
        let ms: Vec<u128> = waits.iter().map(Duration::as_millis).collect();
        assert_eq!(ms, [250, 500, 1000, 2000, 4000, 5000, 5000]);
        assert_eq!(
            backend.opens, 0,
            "no open attempt while the device is absent"
        );
        assert_eq!(machine.attempts(), 7);

        // Replugged: the next due attempt opens it and the machine goes idle.
        backend.present = true;
        now = machine.next_try().expect("waiting");
        assert_eq!(backend.poll(&mut machine, now), ReopenOutcome::Opened(1));
        assert_eq!(machine.state(), ReopenState::Idle);
        assert_eq!(
            backend.poll(&mut machine, now + Duration::from_secs(60)),
            ReopenOutcome::NotDue
        );
    }

    #[test]
    fn a_present_device_that_will_not_open_keeps_backing_off() {
        let t0 = Instant::now();
        let mut machine = Reopener::new();
        let mut backend = mock();
        backend.busy = true;
        machine.device_lost(t0);
        let now = machine.next_try().expect("waiting");
        match backend.poll(&mut machine, now) {
            ReopenOutcome::Failed(AudioError::Build(message)) => {
                assert_eq!(message, "device busy");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(backend.opens, 1);
        assert!(!machine.due(now), "backs off after a failed open");
        assert_eq!(machine.attempts(), 1);

        backend.busy = false;
        let now = machine.next_try().expect("waiting");
        assert_eq!(backend.poll(&mut machine, now), ReopenOutcome::Opened(2));
    }

    #[test]
    fn a_second_loss_while_waiting_keeps_the_schedule() {
        let t0 = Instant::now();
        let mut machine = Reopener::new();
        machine.device_lost(t0);
        machine.failed(t0 + Duration::from_millis(250));
        let before = machine.state();
        machine.device_lost(t0 + Duration::from_secs(1));
        assert_eq!(machine.state(), before);
        machine.cancel();
        assert_eq!(machine.state(), ReopenState::Idle);
    }

    #[test]
    fn backoff_never_overflows() {
        assert_eq!(backoff(0), Duration::from_millis(250));
        assert_eq!(backoff(4), Duration::from_secs(4));
        assert_eq!(backoff(5), REOPEN_MAX);
        assert_eq!(backoff(40), REOPEN_MAX);
        assert_eq!(backoff(u32::MAX), REOPEN_MAX);
    }
}
