//! Output side of the mapping engine: application state to LED and meter
//! messages.
//!
//! [`FeedbackEngine::set`] takes a `(FeedbackTarget, FeedbackValue)` pair from
//! app-core and produces the MIDI messages the mapping assigns to it. The
//! engine owns three pieces of policy:
//!
//! - **De-duplication**: a message is only produced when the data byte for its
//!   `(status, data1)` address differs from the last one sent.
//! - **Rate limiting**: per address, at most one message per interval (lights
//!   1 ms, meters 33 ms by default). A newer value replaces a waiting one, so
//!   the last state always arrives. [`FeedbackEngine::tick`] releases waiting
//!   values and runs timer blinks.
//! - **Shift layers**: entries with `when` are only sent while their modifiers
//!   match; [`FeedbackEngine::set_modifiers`] sends the cached state of entries
//!   that just became active.
//!
//! Nothing here touches the audio thread; app-core calls it from the control
//! thread and the MIDI worker owns the output port.

use std::collections::{HashMap, HashSet};

use opendeck_contracts::control::{FeedbackTarget, FeedbackValue};

use crate::mapping::{Blink, Feedback, Mapping, MessageKind, Output};

/// Rate limit settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeedbackConfig {
    /// Minimum time between messages to one light address, microseconds.
    pub light_min_interval_us: u64,
    /// Minimum time between messages to one level (meter) address.
    pub level_min_interval_us: u64,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            light_min_interval_us: 1_000,
            level_min_interval_us: 33_000,
        }
    }
}

/// An outgoing MIDI message (3 bytes for LED feedback, any length for raw
/// `on_connect` messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutMessage {
    /// Message bytes.
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Addr {
    status: u8,
    data1: u8,
}

#[derive(Debug, Clone, Copy, Default)]
struct Wire {
    /// Last data2 handed to the device.
    sent: Option<u8>,
    /// Newer data2 waiting for the rate limit.
    pending: Option<u8>,
    last_send_us: u64,
}

#[derive(Debug, Clone, Copy)]
struct TimerBlink {
    next_toggle_us: u64,
    lit: bool,
}

/// Feedback state and policy for one mapping.
#[derive(Debug, Clone)]
pub struct FeedbackEngine {
    entries: Vec<Feedback>,
    by_target: HashMap<FeedbackTarget, Vec<usize>>,
    modifier_names: Vec<String>,
    modifier_state: Vec<bool>,
    cache: HashMap<FeedbackTarget, FeedbackValue>,
    wires: HashMap<Addr, Wire>,
    blinks: HashMap<usize, TimerBlink>,
    level_addrs: HashSet<Addr>,
    config: FeedbackConfig,
}

impl FeedbackEngine {
    /// Builds the engine for a mapping.
    pub fn new(mapping: &Mapping, config: FeedbackConfig) -> Self {
        let mut by_target: HashMap<FeedbackTarget, Vec<usize>> = HashMap::new();
        for (i, f) in mapping.feedback.iter().enumerate() {
            by_target.entry(f.target).or_default().push(i);
        }
        let level_addrs = mapping
            .feedback
            .iter()
            .filter(|f| matches!(f.output, Output::Level { .. }))
            .map(|f| Addr {
                status: status_of(f),
                data1: f.midi.number,
            })
            .collect();
        Self {
            entries: mapping.feedback.clone(),
            by_target,
            modifier_names: mapping.modifiers.iter().map(|m| m.id.clone()).collect(),
            modifier_state: vec![false; mapping.modifiers.len()],
            cache: HashMap::new(),
            wires: HashMap::new(),
            blinks: HashMap::new(),
            level_addrs,
            config,
        }
    }

    /// True if the mapping has any output for `target`.
    pub fn handles(&self, target: FeedbackTarget) -> bool {
        self.by_target.contains_key(&target)
    }

    /// Sets the state of `target` at `now_us` and appends the messages that
    /// are due to `out`.
    pub fn set(
        &mut self,
        target: FeedbackTarget,
        value: FeedbackValue,
        now_us: u64,
        out: &mut Vec<OutMessage>,
    ) {
        self.cache.insert(target, value);
        let Some(indices) = self.by_target.get(&target).cloned() else {
            return;
        };
        for i in indices {
            self.apply_entry(i, value, now_us, out);
        }
    }

    /// Updates all modifier states (by `Mapping::modifiers` order) and sends
    /// the cached values of entries that just became active.
    pub fn set_modifiers(&mut self, states: &[bool], now_us: u64, out: &mut Vec<OutMessage>) {
        if states.len() != self.modifier_state.len() {
            return;
        }
        let before = self.modifier_state.clone();
        self.modifier_state.copy_from_slice(states);
        if before == self.modifier_state {
            return;
        }
        for i in 0..self.entries.len() {
            if self.entries[i].when.is_empty() {
                continue;
            }
            let was = self.when_matches(i, &before);
            let now = self.when_matches(i, &self.modifier_state);
            if now
                && !was
                && let Some(value) = self.cache.get(&self.entries[i].target).copied()
            {
                self.apply_entry(i, value, now_us, out);
            }
        }
    }

    /// Releases rate-limited values and toggles timer blinks.
    pub fn tick(&mut self, now_us: u64, out: &mut Vec<OutMessage>) {
        let mut due: Vec<usize> = self
            .blinks
            .iter()
            .filter(|(_, b)| now_us >= b.next_toggle_us)
            .map(|(i, _)| *i)
            .collect();
        due.sort_unstable();
        for i in due {
            let Output::Light {
                on,
                off,
                blink: Some(Blink::Timer { period_ms }),
            } = self.entries[i].output
            else {
                continue;
            };
            if let Some(b) = self.blinks.get_mut(&i) {
                b.lit = !b.lit;
                b.next_toggle_us = now_us + u64::from(period_ms) * 1_000;
                let lit = b.lit;
                self.queue(i, if lit { on } else { off }, now_us, out);
            }
        }
        let mut addrs: Vec<Addr> = self
            .wires
            .iter()
            .filter(|(_, w)| w.pending.is_some())
            .map(|(a, _)| *a)
            .collect();
        addrs.sort_unstable_by_key(|a| (a.status, a.data1));
        for addr in addrs {
            self.release_pending(addr, now_us, out);
        }
    }

    /// Earliest time at which [`FeedbackEngine::tick`] has work, if any.
    pub fn next_deadline_us(&self) -> Option<u64> {
        let blink = self.blinks.values().map(|b| b.next_toggle_us).min();
        let pending = self
            .wires
            .iter()
            .filter(|(_, w)| w.pending.is_some())
            .map(|(a, w)| w.last_send_us + self.interval_for(*a))
            .min();
        match (blink, pending) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Forgets what was sent and sends every cached state again (after the
    /// output reconnects or the device was power-cycled).
    pub fn resend_all(&mut self, now_us: u64, out: &mut Vec<OutMessage>) {
        self.wires.clear();
        // Entry order, so the output is deterministic.
        let todo: Vec<(usize, FeedbackValue)> = (0..self.entries.len())
            .filter_map(|i| Some((i, *self.cache.get(&self.entries[i].target)?)))
            .collect();
        for (i, value) in todo {
            self.apply_entry(i, value, now_us, out);
        }
    }

    /// Forgets the de-duplication state only, so the next `set` of every
    /// target is sent even if unchanged.
    pub fn forget_sent(&mut self) {
        self.wires.clear();
    }

    fn when_matches(&self, i: usize, states: &[bool]) -> bool {
        self.entries[i].when.iter().all(|(name, want)| {
            self.modifier_names
                .iter()
                .position(|n| n == name)
                .is_some_and(|m| states[m] == *want)
        })
    }

    fn interval_for(&self, addr: Addr) -> u64 {
        if self.level_addrs.contains(&addr) {
            self.config.level_min_interval_us
        } else {
            self.config.light_min_interval_us
        }
    }

    fn apply_entry(
        &mut self,
        i: usize,
        value: FeedbackValue,
        now_us: u64,
        out: &mut Vec<OutMessage>,
    ) {
        let entry = &self.entries[i];
        let active = self.when_matches(i, &self.modifier_state);
        let output = entry.output;

        // Timer blink bookkeeping runs even while the entry is inactive so the
        // phase is right when its layer becomes active.
        let timer_period = match output {
            Output::Light {
                blink: Some(Blink::Timer { period_ms }),
                ..
            } => Some(period_ms),
            _ => None,
        };
        if let Some(period_ms) = timer_period {
            if value == FeedbackValue::Blink {
                self.blinks.entry(i).or_insert(TimerBlink {
                    next_toggle_us: now_us + u64::from(period_ms) * 1_000,
                    lit: true,
                });
            } else {
                self.blinks.remove(&i);
            }
        }
        if !active {
            return;
        }
        let data2 = encode(output, value, self.blinks.get(&i).map(|b| b.lit));
        self.queue(i, data2, now_us, out);
    }

    fn queue(&mut self, i: usize, data2: u8, now_us: u64, out: &mut Vec<OutMessage>) {
        let entry = &self.entries[i];
        let addr = Addr {
            status: status_of(entry),
            data1: entry.midi.number,
        };
        let interval = self.interval_for(addr);
        let wire = self.wires.entry(addr).or_default();
        if wire.sent == Some(data2) {
            wire.pending = None;
            return;
        }
        let due = wire.sent.is_none() || now_us.saturating_sub(wire.last_send_us) >= interval;
        if due {
            wire.sent = Some(data2);
            wire.pending = None;
            wire.last_send_us = now_us;
            out.push(OutMessage {
                bytes: vec![addr.status, addr.data1, data2],
            });
        } else {
            wire.pending = Some(data2);
        }
    }

    fn release_pending(&mut self, addr: Addr, now_us: u64, out: &mut Vec<OutMessage>) {
        let interval = self.interval_for(addr);
        let Some(wire) = self.wires.get_mut(&addr) else {
            return;
        };
        let Some(data2) = wire.pending else {
            return;
        };
        if now_us.saturating_sub(wire.last_send_us) < interval {
            return;
        }
        wire.pending = None;
        if wire.sent == Some(data2) {
            return;
        }
        wire.sent = Some(data2);
        wire.last_send_us = now_us;
        out.push(OutMessage {
            bytes: vec![addr.status, addr.data1, data2],
        });
    }
}

fn status_of(entry: &Feedback) -> u8 {
    let base = match entry.midi.kind {
        MessageKind::Note => 0x90,
        MessageKind::Cc => 0xB0,
    };
    base | (entry.midi.channel - 1)
}

/// Data byte for a value. `timer_lit` is the current phase of a timer blink.
fn encode(output: Output, value: FeedbackValue, timer_lit: Option<bool>) -> u8 {
    match output {
        Output::Light { on, off, blink } => match value {
            FeedbackValue::Off => off,
            FeedbackValue::On => on,
            FeedbackValue::Blink => match blink {
                Some(Blink::Device { blink_value }) => blink_value,
                Some(Blink::Timer { .. }) => {
                    if timer_lit.unwrap_or(true) {
                        on
                    } else {
                        off
                    }
                }
                None => on,
            },
            FeedbackValue::Level(x) => {
                if x > 0.0 {
                    on
                } else {
                    off
                }
            }
        },
        Output::Level { min, max } => match value {
            FeedbackValue::Off => min,
            FeedbackValue::On | FeedbackValue::Blink => max,
            FeedbackValue::Level(x) => {
                let x = if x.is_finite() {
                    x.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let span = f32::from(max) - f32::from(min);
                (f32::from(min) + span * x).round().clamp(0.0, 127.0) as u8
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::ids::DeckId;

    fn deck(i: u8) -> DeckId {
        DeckId::new(i).expect("deck")
    }

    const MAP: &str = r#"{
        "format_version": 1, "id": "test", "version": 1, "name": "T", "source": "t",
        "devices": { "input": ["T"] },
        "modifiers": [{ "id": "shift", "midi": { "type": "note", "channel": 1, "number": 63 } }],
        "controls": [],
        "feedback": [
          { "target": { "target": "playing", "deck": 0 }, "midi": { "type": "note", "channel": 1, "number": 11 },
            "output": { "kind": "light", "on": 127, "off": 0, "blink": { "mode": "device", "blink_value": 5 } } },
          { "target": { "target": "cue_indicator", "deck": 0 }, "midi": { "type": "note", "channel": 1, "number": 12 },
            "when": { "shift": false }, "output": { "kind": "light", "on": 127, "off": 0 } },
          { "target": { "target": "cue_indicator", "deck": 0 }, "midi": { "type": "note", "channel": 1, "number": 72 },
            "when": { "shift": true }, "output": { "kind": "light", "on": 127, "off": 0 } },
          { "target": { "target": "sync_on", "deck": 0 }, "midi": { "type": "note", "channel": 2, "number": 88 },
            "output": { "kind": "light", "on": 127, "off": 0, "blink": { "mode": "timer", "period_ms": 100 } } },
          { "target": { "target": "channel_level", "deck": 0 }, "midi": { "type": "cc", "channel": 1, "number": 2 },
            "output": { "kind": "level", "min": 0, "max": 127 } }
        ]
    }"#;

    fn engine() -> FeedbackEngine {
        let mapping = Mapping::from_json(MAP).expect("map");
        FeedbackEngine::new(&mapping, FeedbackConfig::default())
    }

    fn set(e: &mut FeedbackEngine, t: FeedbackTarget, v: FeedbackValue, now: u64) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        e.set(t, v, now, &mut out);
        out.into_iter().map(|m| m.bytes).collect()
    }

    #[test]
    fn light_on_off_and_dedup() {
        let mut e = engine();
        let t = FeedbackTarget::Playing { deck: deck(0) };
        assert_eq!(
            set(&mut e, t, FeedbackValue::On, 0),
            vec![vec![0x90, 11, 127]]
        );
        assert!(set(&mut e, t, FeedbackValue::On, 10_000).is_empty());
        assert_eq!(
            set(&mut e, t, FeedbackValue::Off, 20_000),
            vec![vec![0x90, 11, 0]]
        );
    }

    #[test]
    fn device_blink_sends_blink_value() {
        let mut e = engine();
        let t = FeedbackTarget::Playing { deck: deck(0) };
        assert_eq!(
            set(&mut e, t, FeedbackValue::Blink, 0),
            vec![vec![0x90, 11, 5]]
        );
    }

    #[test]
    fn level_scales_and_rate_limits_to_last_value() {
        let mut e = engine();
        let t = FeedbackTarget::ChannelLevel { deck: deck(0) };
        assert_eq!(
            set(&mut e, t, FeedbackValue::Level(0.5), 0),
            vec![vec![0xB0, 2, 64]]
        );
        assert!(set(&mut e, t, FeedbackValue::Level(0.6), 10_000).is_empty());
        assert!(set(&mut e, t, FeedbackValue::Level(1.0), 20_000).is_empty());
        let mut out = Vec::new();
        e.tick(32_000, &mut out);
        assert!(out.is_empty(), "interval not over");
        assert_eq!(e.next_deadline_us(), Some(33_000));
        e.tick(33_000, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bytes, vec![0xB0, 2, 127]);
        assert_eq!(e.next_deadline_us(), None);
    }

    #[test]
    fn level_out_of_range_is_clamped() {
        let mut e = engine();
        let t = FeedbackTarget::ChannelLevel { deck: deck(0) };
        assert_eq!(
            set(&mut e, t, FeedbackValue::Level(7.0), 0),
            vec![vec![0xB0, 2, 127]]
        );
        assert_eq!(
            set(&mut e, t, FeedbackValue::Level(f32::NAN), 100_000),
            vec![vec![0xB0, 2, 0]]
        );
    }

    #[test]
    fn when_selects_the_shift_layer_and_resends_on_change() {
        let mut e = engine();
        let t = FeedbackTarget::CueIndicator { deck: deck(0) };
        // Shift released: only the base entry is sent.
        assert_eq!(
            set(&mut e, t, FeedbackValue::On, 0),
            vec![vec![0x90, 12, 127]]
        );
        // Shift pressed: the shifted entry gets the cached state.
        let mut out = Vec::new();
        e.set_modifiers(&[true], 5_000, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bytes, vec![0x90, 72, 127]);
        // State change while shifted goes to the shifted address only.
        assert_eq!(
            set(&mut e, t, FeedbackValue::Off, 10_000),
            vec![vec![0x90, 72, 0]]
        );
        // Releasing shift sends the base address its current (off) state.
        let mut out = Vec::new();
        e.set_modifiers(&[false], 20_000, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bytes, vec![0x90, 12, 0]);
    }

    #[test]
    fn timer_blink_toggles_on_tick() {
        let mut e = engine();
        let t = FeedbackTarget::SyncOn { deck: deck(0) };
        assert_eq!(
            set(&mut e, t, FeedbackValue::Blink, 0),
            vec![vec![0x91, 88, 127]]
        );
        assert_eq!(e.next_deadline_us(), Some(100_000));
        let mut out = Vec::new();
        e.tick(100_000, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bytes, vec![0x91, 88, 0]);
        let mut out = Vec::new();
        e.tick(200_000, &mut out);
        assert_eq!(out[0].bytes, vec![0x91, 88, 127]);
        // Back to steady on stops the toggling.
        assert!(set(&mut e, t, FeedbackValue::On, 210_000).is_empty());
        let mut out = Vec::new();
        e.tick(400_000, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn resend_all_replays_the_cache() {
        let mut e = engine();
        let t = FeedbackTarget::Playing { deck: deck(0) };
        set(&mut e, t, FeedbackValue::On, 0);
        let mut out = Vec::new();
        e.resend_all(1_000_000, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].bytes, vec![0x90, 11, 127]);
    }

    #[test]
    fn unknown_target_is_ignored() {
        let mut e = engine();
        let t = FeedbackTarget::PflOn { deck: deck(0) };
        assert!(!e.handles(t));
        assert!(set(&mut e, t, FeedbackValue::On, 0).is_empty());
    }
}
