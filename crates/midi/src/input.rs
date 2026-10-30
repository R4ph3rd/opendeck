//! Input side of the mapping engine: raw MIDI bytes to [`ControlEvent`]s.
//!
//! The engine is a plain state machine without threads or clocks: callers
//! pass the driver timestamp with every message and call [`MappingEngine::tick`]
//! to flush 14-bit MSB-only updates. That makes it deterministic for the
//! replay harness and cheap to run on the MIDI worker thread.
//!
//! # 14-bit controls
//!
//! The message list gives MSB/LSB CC pairs but not their send order. The
//! engine keeps the last MSB and LSB per control and:
//! - emits on every LSB, combining it with the last MSB;
//! - after an MSB, waits `msb_timeout_us` (default 5 ms) for an LSB and emits
//!   the MSB with the last LSB if none comes.
//!
//! Identical consecutive values are not emitted twice.
//!
//! # Soft takeover
//!
//! The mapping layer reports raw absolute positions. It does not hide
//! hardware/software mismatches: the FLX4 sends no position dump on connect, so
//! a control's value is unknown until it moves. Pickup or takeover policy
//! (ignore a fader until it crosses the software value) is app-core's concern;
//! [`MappingEngine::last_absolute`] exposes the last reported position per
//! control for it.

use std::collections::HashMap;

use opendeck_contracts::control::{ControlEvent, ControlValue};

use crate::error::MappingError;
use crate::mapping::{Edge, Mapping, MessageKind, MidiInput, ValueSpec};
use crate::message::MidiMessage;

/// Default time to wait for an LSB after an MSB.
pub const DEFAULT_MSB_TIMEOUT_US: u64 = 5_000;

/// What the engine did with one message (for the monitor and the learn mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handled {
    /// A control matched; `events` events were produced.
    Control {
        /// Index into `Mapping::controls`.
        index: usize,
        /// Events pushed to the output (0 when filtered by edge or dedup).
        events: usize,
    },
    /// A modifier changed.
    Modifier {
        /// Index into `Mapping::modifiers`.
        index: usize,
        /// New state.
        active: bool,
    },
    /// An MSB or LSB was stored and no event came out yet.
    Pending {
        /// Index into `Mapping::controls`.
        index: usize,
    },
    /// A known channel message without a control.
    Unmapped,
    /// Not a channel message the mapping format covers (system, aftertouch).
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    cc: bool,
    channel: u8,
    number: u8,
}

#[derive(Debug, Clone, Copy)]
enum Role {
    Plain,
    Msb,
    Lsb,
}

#[derive(Debug, Clone)]
struct Slot {
    control: usize,
    role: Role,
    /// Required modifier states as (modifier index, state).
    when: Vec<(usize, bool)>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Pair14 {
    msb: u8,
    lsb: u8,
    /// Time of an MSB that still waits for its LSB.
    pending_since: Option<u64>,
    last_emitted: Option<u16>,
}

/// Turns MIDI bytes into [`ControlEvent`]s according to a [`Mapping`].
#[derive(Debug, Clone)]
pub struct MappingEngine {
    mapping: Mapping,
    slots: HashMap<Key, Vec<Slot>>,
    modifier_keys: HashMap<Key, usize>,
    modifier_state: Vec<bool>,
    pairs: HashMap<usize, Pair14>,
    last_absolute: HashMap<usize, f32>,
    msb_timeout_us: u64,
}

impl MappingEngine {
    /// Builds the lookup tables. Validates the mapping.
    pub fn new(mapping: Mapping) -> Result<Self, MappingError> {
        mapping.validate()?;
        let modifier_index: HashMap<&str, usize> = mapping
            .modifiers
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id.as_str(), i))
            .collect();
        let mut slots: HashMap<Key, Vec<Slot>> = HashMap::new();
        for (i, control) in mapping.controls.iter().enumerate() {
            let when: Vec<(usize, bool)> = control
                .when
                .iter()
                .map(|(k, v)| (modifier_index[k.as_str()], *v))
                .collect();
            let mut add = |key: Key, role: Role| {
                slots.entry(key).or_default().push(Slot {
                    control: i,
                    role,
                    when: when.clone(),
                });
            };
            match control.midi {
                MidiInput::Note { channel, number } => add(
                    Key {
                        cc: false,
                        channel: channel - 1,
                        number,
                    },
                    Role::Plain,
                ),
                MidiInput::Cc { channel, number } => add(
                    Key {
                        cc: true,
                        channel: channel - 1,
                        number,
                    },
                    Role::Plain,
                ),
                MidiInput::Cc14 { channel, msb, lsb } => {
                    add(
                        Key {
                            cc: true,
                            channel: channel - 1,
                            number: msb,
                        },
                        Role::Msb,
                    );
                    add(
                        Key {
                            cc: true,
                            channel: channel - 1,
                            number: lsb,
                        },
                        Role::Lsb,
                    );
                }
            }
        }
        // More specific entries (more `when` terms) are tried first.
        for list in slots.values_mut() {
            list.sort_by_key(|s| std::cmp::Reverse(s.when.len()));
        }
        let modifier_keys = mapping
            .modifiers
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    Key {
                        cc: m.midi.kind == MessageKind::Cc,
                        channel: m.midi.channel - 1,
                        number: m.midi.number,
                    },
                    i,
                )
            })
            .collect();
        Ok(Self {
            modifier_state: vec![false; mapping.modifiers.len()],
            mapping,
            slots,
            modifier_keys,
            pairs: HashMap::new(),
            last_absolute: HashMap::new(),
            msb_timeout_us: DEFAULT_MSB_TIMEOUT_US,
        })
    }

    /// Sets the wait for an LSB after an MSB.
    pub fn set_msb_timeout_us(&mut self, us: u64) {
        self.msb_timeout_us = us;
    }

    /// The mapping this engine runs.
    pub fn mapping(&self) -> &Mapping {
        &self.mapping
    }

    /// State of a modifier by id (false for unknown ids).
    pub fn modifier_active(&self, id: &str) -> bool {
        self.mapping
            .modifiers
            .iter()
            .position(|m| m.id == id)
            .is_some_and(|i| self.modifier_state[i])
    }

    /// Modifier states by index into `Mapping::modifiers`.
    pub fn modifier_states(&self) -> &[bool] {
        &self.modifier_state
    }

    /// Last absolute position reported for the control at `index`, for
    /// app-core's soft takeover.
    pub fn last_absolute(&self, index: usize) -> Option<f32> {
        self.last_absolute.get(&index).copied()
    }

    /// Earliest time at which [`MappingEngine::tick`] has work, if any.
    pub fn next_deadline_us(&self) -> Option<u64> {
        self.pairs
            .values()
            .filter_map(|p| p.pending_since)
            .map(|t| t + self.msb_timeout_us)
            .min()
    }

    /// Emits MSB-only updates whose LSB did not arrive in time.
    pub fn tick(&mut self, now_us: u64, out: &mut Vec<ControlEvent>) {
        let expired: Vec<usize> = self
            .pairs
            .iter()
            .filter(|(_, p)| {
                p.pending_since
                    .is_some_and(|t| now_us.saturating_sub(t) >= self.msb_timeout_us)
            })
            .map(|(i, _)| *i)
            .collect();
        for index in expired {
            let pair = self.pairs.entry(index).or_default();
            let since = pair.pending_since.take().unwrap_or(now_us);
            self.emit_14(index, since + self.msb_timeout_us, out);
        }
    }

    /// Processes one raw message received at `ts_us`.
    pub fn process(&mut self, ts_us: u64, bytes: &[u8], out: &mut Vec<ControlEvent>) -> Handled {
        self.tick(ts_us, out);
        let Some(message) = MidiMessage::parse(bytes) else {
            return Handled::Ignored;
        };
        let (key, raw, is_off) = match message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => (
                Key {
                    cc: false,
                    channel,
                    number: note,
                },
                velocity,
                false,
            ),
            MidiMessage::NoteOff {
                channel,
                note,
                velocity,
            } => (
                Key {
                    cc: false,
                    channel,
                    number: note,
                },
                velocity,
                true,
            ),
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => (
                Key {
                    cc: true,
                    channel,
                    number: controller,
                },
                value,
                false,
            ),
            MidiMessage::PitchBend { .. } | MidiMessage::ProgramChange { .. } => {
                return Handled::Ignored;
            }
        };

        if let Some(&index) = self.modifier_keys.get(&key) {
            let active = !is_off && raw > 0;
            self.modifier_state[index] = active;
            return Handled::Modifier { index, active };
        }

        let Some(candidates) = self.slots.get(&key) else {
            return Handled::Unmapped;
        };
        let state = &self.modifier_state;
        let Some(slot) = candidates
            .iter()
            .find(|s| s.when.iter().all(|(m, want)| state[*m] == *want))
            .cloned()
        else {
            return Handled::Unmapped;
        };

        let before = out.len();
        match slot.role {
            Role::Plain => self.plain(slot.control, ts_us, raw, is_off, out),
            Role::Msb => {
                let pair = self.pairs.entry(slot.control).or_default();
                pair.msb = raw;
                pair.pending_since = Some(ts_us);
                return Handled::Pending {
                    index: slot.control,
                };
            }
            Role::Lsb => {
                let pair = self.pairs.entry(slot.control).or_default();
                pair.lsb = raw;
                pair.pending_since = None;
                self.emit_14(slot.control, ts_us, out);
            }
        }
        Handled::Control {
            index: slot.control,
            events: out.len() - before,
        }
    }

    fn plain(
        &mut self,
        index: usize,
        ts_us: u64,
        raw: u8,
        is_off: bool,
        out: &mut Vec<ControlEvent>,
    ) {
        let control = &self.mapping.controls[index];
        let value = match control.value {
            ValueSpec::Button {
                threshold,
                edge,
                invert,
            } => {
                let pressed = !is_off && raw >= threshold.unwrap_or(1).max(1);
                let wanted = match edge {
                    Edge::Both => true,
                    Edge::Press => pressed,
                    Edge::Release => !pressed,
                };
                if !wanted {
                    return;
                }
                ControlValue::Button(pressed != invert)
            }
            ValueSpec::Absolute { invert } => {
                let x = f32::from(raw) / 127.0;
                let x = if invert { 1.0 - x } else { x };
                self.last_absolute.insert(index, x);
                ControlValue::Absolute(x)
            }
            ValueSpec::Relative { encoding, scale } => {
                let delta = encoding.delta(raw);
                if delta == 0 {
                    return;
                }
                ControlValue::Relative(f32::from(delta) * scale)
            }
        };
        out.push(ControlEvent {
            action: control.action,
            value,
            timestamp_us: ts_us,
        });
    }

    fn emit_14(&mut self, index: usize, ts_us: u64, out: &mut Vec<ControlEvent>) {
        let invert = matches!(
            self.mapping.controls[index].value,
            ValueSpec::Absolute { invert: true }
        );
        let action = self.mapping.controls[index].action;
        let pair = self.pairs.entry(index).or_default();
        let combined = u16::from(pair.msb) * 128 + u16::from(pair.lsb);
        if pair.last_emitted == Some(combined) {
            return;
        }
        pair.last_emitted = Some(combined);
        let x = f32::from(combined) / 16383.0;
        let x = if invert { 1.0 - x } else { x };
        self.last_absolute.insert(index, x);
        out.push(ControlEvent {
            action,
            value: ControlValue::Absolute(x),
            timestamp_us: ts_us,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::control::ControlAction;
    use opendeck_contracts::ids::DeckId;

    fn deck(i: u8) -> DeckId {
        DeckId::new(i).expect("deck")
    }

    const MAP: &str = r#"{
        "format_version": 1, "id": "test", "version": 1, "name": "T", "source": "t",
        "devices": { "input": ["T"] },
        "modifiers": [{ "id": "shift", "midi": { "type": "note", "channel": 1, "number": 63 } }],
        "controls": [
          { "id": "play", "midi": { "type": "note", "channel": 1, "number": 11 },
            "action": { "action": "play_pause", "deck": 0 }, "value": { "kind": "button" } },
          { "id": "play.shift", "midi": { "type": "note", "channel": 1, "number": 11 }, "when": { "shift": true },
            "action": { "action": "return_to_start", "deck": 0 }, "value": { "kind": "button", "edge": "press" } },
          { "id": "sync", "midi": { "type": "note", "channel": 1, "number": 88 },
            "action": { "action": "sync", "deck": 0 }, "value": { "kind": "button", "edge": "press", "invert": true } },
          { "id": "tempo", "midi": { "type": "cc14", "channel": 1, "msb": 0, "lsb": 32 },
            "action": { "action": "tempo_fader", "deck": 0 }, "value": { "kind": "absolute" } },
          { "id": "jog", "midi": { "type": "cc", "channel": 1, "number": 34 },
            "action": { "action": "jog_top", "deck": 0 }, "value": { "kind": "relative", "encoding": "offset64", "scale": 0.5 } },
          { "id": "browse", "midi": { "type": "cc", "channel": 7, "number": 64 },
            "action": { "action": "browse_scroll" }, "value": { "kind": "relative", "encoding": "twos_complement" } },
          { "id": "xfade", "midi": { "type": "cc", "channel": 7, "number": 9 },
            "action": { "action": "crossfader" }, "value": { "kind": "absolute", "invert": true } }
        ]
    }"#;

    fn engine() -> MappingEngine {
        MappingEngine::new(Mapping::from_json(MAP).expect("map")).expect("engine")
    }

    fn feed(e: &mut MappingEngine, ts: u64, bytes: &[u8]) -> (Handled, Vec<ControlEvent>) {
        let mut out = Vec::new();
        let h = e.process(ts, bytes, &mut out);
        (h, out)
    }

    #[test]
    fn button_press_and_release() {
        let mut e = engine();
        let (_, on) = feed(&mut e, 10, &[0x90, 11, 127]);
        assert_eq!(
            on,
            vec![ControlEvent {
                action: ControlAction::PlayPause { deck: deck(0) },
                value: ControlValue::Button(true),
                timestamp_us: 10
            }]
        );
        let (_, off) = feed(&mut e, 20, &[0x90, 11, 0]);
        assert_eq!(off[0].value, ControlValue::Button(false));
        let (_, off) = feed(&mut e, 30, &[0x80, 11, 0]);
        assert_eq!(off[0].value, ControlValue::Button(false));
    }

    #[test]
    fn modifier_selects_the_when_entry() {
        let mut e = engine();
        let (h, _) = feed(&mut e, 0, &[0x90, 63, 127]);
        assert!(matches!(h, Handled::Modifier { active: true, .. }));
        assert!(e.modifier_active("shift"));
        let (_, out) = feed(&mut e, 1, &[0x90, 11, 127]);
        assert_eq!(
            out[0].action,
            ControlAction::ReturnToStart { deck: deck(0) }
        );
        let (_, release) = feed(&mut e, 2, &[0x90, 11, 0]);
        assert!(release.is_empty(), "edge press drops the release");
        feed(&mut e, 3, &[0x90, 63, 0]);
        assert!(!e.modifier_active("shift"));
        let (_, out) = feed(&mut e, 4, &[0x90, 11, 127]);
        assert_eq!(out[0].action, ControlAction::PlayPause { deck: deck(0) });
    }

    #[test]
    fn invert_and_edge() {
        let mut e = engine();
        let (_, press) = feed(&mut e, 0, &[0x90, 88, 127]);
        assert_eq!(press[0].value, ControlValue::Button(false));
        let (h, release) = feed(&mut e, 1, &[0x90, 88, 0]);
        assert!(release.is_empty());
        assert!(matches!(h, Handled::Control { events: 0, .. }));
    }

    #[test]
    fn relative_encodings() {
        let mut e = engine();
        let (_, cw) = feed(&mut e, 0, &[0xB0, 34, 0x41]);
        assert_eq!(cw[0].value, ControlValue::Relative(0.5));
        let (_, ccw) = feed(&mut e, 1, &[0xB0, 34, 0x3E]);
        assert_eq!(ccw[0].value, ControlValue::Relative(-1.0));
        let (_, none) = feed(&mut e, 2, &[0xB0, 34, 0x40]);
        assert!(none.is_empty());
        let (_, up) = feed(&mut e, 3, &[0xB6, 64, 0x01]);
        assert_eq!(up[0].value, ControlValue::Relative(1.0));
        let (_, down) = feed(&mut e, 4, &[0xB6, 64, 0x7F]);
        assert_eq!(down[0].value, ControlValue::Relative(-1.0));
    }

    #[test]
    fn absolute_invert_and_last_value() {
        let mut e = engine();
        let (_, out) = feed(&mut e, 0, &[0xB6, 9, 0]);
        assert_eq!(out[0].value, ControlValue::Absolute(1.0));
        let idx = e
            .mapping()
            .controls
            .iter()
            .position(|c| c.id == "xfade")
            .expect("control");
        assert_eq!(e.last_absolute(idx), Some(1.0));
    }

    #[test]
    fn cc14_msb_then_lsb_emits_once() {
        let mut e = engine();
        let (h, out) = feed(&mut e, 0, &[0xB0, 0, 0x7F]);
        assert!(matches!(h, Handled::Pending { .. }));
        assert!(out.is_empty());
        let (_, out) = feed(&mut e, 100, &[0xB0, 32, 0x7F]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].value, ControlValue::Absolute(1.0));
        let mut late = Vec::new();
        e.tick(10_000, &mut late);
        assert!(late.is_empty(), "no second event after the pair completed");
    }

    #[test]
    fn cc14_msb_alone_emits_after_timeout() {
        let mut e = engine();
        feed(&mut e, 0, &[0xB0, 0, 0x40]);
        assert_eq!(e.next_deadline_us(), Some(DEFAULT_MSB_TIMEOUT_US));
        let mut out = Vec::new();
        e.tick(4_999, &mut out);
        assert!(out.is_empty());
        e.tick(5_000, &mut out);
        assert_eq!(out.len(), 1);
        let ControlValue::Absolute(x) = out[0].value else {
            panic!("absolute")
        };
        assert!((x - 8192.0 / 16383.0).abs() < 1e-6);
        assert_eq!(e.next_deadline_us(), None);
    }

    #[test]
    fn cc14_lsb_first_converges() {
        let mut e = engine();
        // LSB first, then MSB: the stale pair emits, the timeout emits the corrected value.
        feed(&mut e, 0, &[0xB0, 32, 0x10]);
        feed(&mut e, 10, &[0xB0, 0, 0x20]);
        let mut out = Vec::new();
        e.tick(6_000, &mut out);
        let ControlValue::Absolute(x) = out[0].value else {
            panic!("absolute")
        };
        assert!((x - f32::from(0x20u16 * 128 + 0x10) / 16383.0).abs() < 1e-6);
    }

    #[test]
    fn cc14_ignores_repeated_value() {
        let mut e = engine();
        feed(&mut e, 0, &[0xB0, 0, 5]);
        let (_, a) = feed(&mut e, 1, &[0xB0, 32, 9]);
        let (_, b) = feed(&mut e, 2, &[0xB0, 32, 9]);
        assert_eq!(a.len(), 1);
        assert!(b.is_empty());
    }

    #[test]
    fn unknown_and_system_messages() {
        let mut e = engine();
        assert_eq!(feed(&mut e, 0, &[0x95, 1, 1]).0, Handled::Unmapped);
        assert_eq!(feed(&mut e, 0, &[0xF8]).0, Handled::Ignored);
        assert_eq!(feed(&mut e, 0, &[0xE0, 0, 0x40]).0, Handled::Ignored);
    }
}
