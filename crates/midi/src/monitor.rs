//! MIDI monitor: raw traffic in both directions with timestamps and what the
//! mapping made of it.
//!
//! The controller worker sends [`MonitorEvent`]s on a bounded channel when the
//! monitor is enabled (`Controller::set_monitor`). The shell shows them,
//! highlights `verify` values and offers MIDI learn on unmapped messages.

use std::collections::HashMap;

use crate::input::Handled;
use crate::mapping::{Mapping, MidiInput};
use crate::message::{MidiMessage, describe};

/// Direction of a monitored message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// From the controller to OpenDeck.
    In,
    /// From OpenDeck to the controller (LED feedback, `on_connect`).
    Out,
}

/// One monitored message.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorEvent {
    /// Timestamp, microseconds on the worker clock (monotonic, arbitrary origin).
    pub ts_us: u64,
    /// In or out.
    pub direction: Direction,
    /// The raw bytes.
    pub bytes: Vec<u8>,
    /// Readable form, e.g. `Note On ch1 0x0B vel 127`.
    pub text: String,
    /// What the mapping did with an input message.
    pub label: MonitorLabel,
}

/// The mapping's reading of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorLabel {
    /// Output message, nothing to label.
    Output,
    /// Input matched a control.
    Control {
        /// Control id.
        id: String,
        /// The control's `verify` note, to highlight.
        verify: Option<String>,
    },
    /// Input matched a modifier.
    Modifier {
        /// Modifier id.
        id: String,
        /// The modifier's `verify` note.
        verify: Option<String>,
    },
    /// Half of a 14-bit pair, waiting for the other half.
    Pending {
        /// Control id.
        id: String,
    },
    /// The source document lists the message but the mapping leaves it unmapped.
    KnownUnmapped {
        /// Source row.
        source_ref: String,
        /// Why there is no action.
        reason: String,
    },
    /// Not in the mapping at all (candidate for MIDI learn).
    Unmapped,
    /// Not a channel message the mapping covers.
    Ignored,
}

/// Looks up labels for input messages. Built once per mapping.
#[derive(Debug, Clone)]
pub struct MonitorLabeler {
    known_unmapped: HashMap<(bool, u8, u8), (String, String)>,
    controls: Vec<(String, Option<String>)>,
    modifiers: Vec<(String, Option<String>)>,
}

impl MonitorLabeler {
    /// Indexes a mapping.
    pub fn new(mapping: &Mapping) -> Self {
        let mut known_unmapped = HashMap::new();
        for u in &mapping.unmapped {
            let (cc, channel, numbers) = match u.midi {
                MidiInput::Note { channel, number } => (false, channel, vec![number]),
                MidiInput::Cc { channel, number } => (true, channel, vec![number]),
                MidiInput::Cc14 { channel, msb, lsb } => (true, channel, vec![msb, lsb]),
            };
            for n in numbers {
                known_unmapped.insert(
                    (cc, channel - 1, n),
                    (u.source_ref.clone(), u.reason.clone()),
                );
            }
        }
        Self {
            known_unmapped,
            controls: mapping
                .controls
                .iter()
                .map(|c| (c.id.clone(), c.verify.clone()))
                .collect(),
            modifiers: mapping
                .modifiers
                .iter()
                .map(|m| (m.id.clone(), m.verify.clone()))
                .collect(),
        }
    }

    /// Builds the monitor event for a received message.
    pub fn input(&self, ts_us: u64, bytes: &[u8], handled: &Handled) -> MonitorEvent {
        let label = match handled {
            Handled::Control { index, .. } => {
                let (id, verify) = &self.controls[*index];
                MonitorLabel::Control {
                    id: id.clone(),
                    verify: verify.clone(),
                }
            }
            Handled::Modifier { index, .. } => {
                let (id, verify) = &self.modifiers[*index];
                MonitorLabel::Modifier {
                    id: id.clone(),
                    verify: verify.clone(),
                }
            }
            Handled::Pending { index } => MonitorLabel::Pending {
                id: self.controls[*index].0.clone(),
            },
            Handled::Ignored => MonitorLabel::Ignored,
            Handled::Unmapped => match self.known(bytes) {
                Some((source_ref, reason)) => MonitorLabel::KnownUnmapped { source_ref, reason },
                None => MonitorLabel::Unmapped,
            },
        };
        MonitorEvent {
            ts_us,
            direction: Direction::In,
            bytes: bytes.to_vec(),
            text: describe(bytes),
            label,
        }
    }

    /// Builds the monitor event for a sent message.
    pub fn output(&self, ts_us: u64, bytes: &[u8]) -> MonitorEvent {
        MonitorEvent {
            ts_us,
            direction: Direction::Out,
            bytes: bytes.to_vec(),
            text: describe(bytes),
            label: MonitorLabel::Output,
        }
    }

    fn known(&self, bytes: &[u8]) -> Option<(String, String)> {
        let key = match MidiMessage::parse(bytes)? {
            MidiMessage::NoteOn { channel, note, .. }
            | MidiMessage::NoteOff { channel, note, .. } => (false, channel, note),
            MidiMessage::ControlChange {
                channel,
                controller,
                ..
            } => (true, channel, controller),
            _ => return None,
        };
        self.known_unmapped.get(&key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"{
        "format_version": 1, "id": "test", "version": 1, "name": "T", "source": "t",
        "devices": { "input": ["T"] },
        "controls": [
          { "id": "play", "midi": { "type": "note", "channel": 1, "number": 11 }, "verify": "Q99 check",
            "action": { "action": "play_pause", "deck": 0 }, "value": { "kind": "button" } }
        ],
        "unmapped": [
          { "ref": "9-9", "label": "MIC", "midi": { "type": "cc14", "channel": 7, "msb": 5, "lsb": 37 }, "reason": "no mic input" }
        ]
    }"#;

    #[test]
    fn labels() {
        let mapping = Mapping::from_json(MAP).expect("map");
        let labeler = MonitorLabeler::new(&mapping);
        let e = labeler.input(
            7,
            &[0x90, 11, 127],
            &Handled::Control {
                index: 0,
                events: 1,
            },
        );
        assert_eq!(e.text, "Note On ch1 0x0B vel 127");
        assert_eq!(
            e.label,
            MonitorLabel::Control {
                id: "play".into(),
                verify: Some("Q99 check".into())
            }
        );
        let e = labeler.input(8, &[0xB6, 5, 3], &Handled::Unmapped);
        assert_eq!(
            e.label,
            MonitorLabel::KnownUnmapped {
                source_ref: "9-9".into(),
                reason: "no mic input".into()
            }
        );
        let e = labeler.input(9, &[0xB6, 6, 3], &Handled::Unmapped);
        assert_eq!(e.label, MonitorLabel::Unmapped);
        let e = labeler.output(10, &[0x90, 11, 0]);
        assert_eq!(e.direction, Direction::Out);
    }
}
