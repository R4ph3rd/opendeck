//! MIDI learn primitives: capture the next message and build a mapping entry.
//!
//! The shell arms a [`LearnSession`] for a chosen action; the next gesture on
//! the controller becomes a [`LearnCapture`] (input address and value kind),
//! and [`build_control`] turns it into a [`Control`] that
//! [`Mapping::upsert_control`] adds or replaces. The result can be stored as a
//! user override (ADR 0005, `midi_mapping_overrides`).
//!
//! Detection:
//! - a note on starts a button immediately;
//! - control changes are collected for a short window (default 150 ms) because
//!   one message cannot tell an absolute knob from a relative encoder or an
//!   MSB from a 14-bit pair. Within the window an MSB CC (below 32) followed by
//!   its LSB (+32) on the same channel is a 14-bit control; values all in
//!   `{1, 2, 3, 125, 126, 127}` are a twos-complement encoder; all in
//!   `61..=67` an offset-64 encoder; anything else is 7-bit absolute.
//! - a [`LearnHint`] overrides the guess.

use std::collections::BTreeMap;

use opendeck_contracts::control::ControlAction;

use crate::error::MappingError;
use crate::mapping::{Control, Edge, Mapping, MidiInput, RelativeEncoding, ValueSpec};
use crate::message::{MidiMessage, describe};

/// Default CC collection window.
pub const DEFAULT_LEARN_WINDOW_US: u64 = 150_000;

/// What the caller expects the control to be.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LearnHint {
    /// Guess from the messages.
    #[default]
    Auto,
    /// A button, also when it is sent as a CC.
    Button,
    /// An absolute knob or fader.
    Absolute,
    /// A relative encoder with this encoding.
    Relative(RelativeEncoding),
}

/// The captured gesture.
#[derive(Debug, Clone, PartialEq)]
pub struct LearnCapture {
    /// Where the control sends.
    pub midi: MidiInput,
    /// How its data byte reads.
    pub value: ValueSpec,
    /// Messages that contributed.
    pub messages: usize,
    /// Readable form of the first contributing message.
    pub description: String,
}

#[derive(Debug, Default)]
struct CcStats {
    values: Vec<u8>,
    first_ts: u64,
}

/// Captures the next gesture. Feed it every received message while armed.
#[derive(Debug)]
pub struct LearnSession {
    hint: LearnHint,
    window_us: u64,
    window_start: Option<u64>,
    ccs: BTreeMap<(u8, u8), CcStats>,
    first_description: Option<String>,
    done: bool,
}

impl LearnSession {
    /// A session with the default window.
    pub fn new(hint: LearnHint) -> Self {
        Self::with_window(hint, DEFAULT_LEARN_WINDOW_US)
    }

    /// A session with an explicit CC window.
    pub fn with_window(hint: LearnHint, window_us: u64) -> Self {
        Self {
            hint,
            window_us,
            window_start: None,
            ccs: BTreeMap::new(),
            first_description: None,
            done: false,
        }
    }

    /// True once a capture was returned.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Time at which [`LearnSession::tick`] will finish a started CC window.
    pub fn deadline_us(&self) -> Option<u64> {
        if self.done {
            return None;
        }
        self.window_start.map(|s| s + self.window_us)
    }

    /// Feeds one received message. Returns the capture when it is complete.
    pub fn feed(&mut self, ts_us: u64, bytes: &[u8]) -> Option<LearnCapture> {
        if self.done {
            return None;
        }
        if let Some(capture) = self.tick(ts_us) {
            return Some(capture);
        }
        let message = MidiMessage::parse(bytes)?;
        match message {
            MidiMessage::NoteOn { channel, note, .. } => {
                self.done = true;
                Some(LearnCapture {
                    midi: MidiInput::Note {
                        channel: channel + 1,
                        number: note,
                    },
                    value: ValueSpec::Button {
                        threshold: None,
                        edge: Edge::Both,
                        invert: false,
                    },
                    messages: 1,
                    description: describe(bytes),
                })
            }
            // A note-off alone is the end of an earlier press, not a gesture.
            MidiMessage::NoteOff { .. } => None,
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => {
                if self.hint == LearnHint::Button {
                    self.done = true;
                    return Some(LearnCapture {
                        midi: MidiInput::Cc {
                            channel: channel + 1,
                            number: controller,
                        },
                        value: ValueSpec::Button {
                            threshold: None,
                            edge: Edge::Both,
                            invert: false,
                        },
                        messages: 1,
                        description: describe(bytes),
                    });
                }
                self.window_start.get_or_insert(ts_us);
                self.first_description
                    .get_or_insert_with(|| describe(bytes));
                let stats = self
                    .ccs
                    .entry((channel, controller))
                    .or_insert_with(|| CcStats {
                        values: Vec::new(),
                        first_ts: ts_us,
                    });
                stats.values.push(value);
                None
            }
            MidiMessage::PitchBend { .. } | MidiMessage::ProgramChange { .. } => None,
        }
    }

    /// Finishes the CC window if it elapsed.
    pub fn tick(&mut self, now_us: u64) -> Option<LearnCapture> {
        if self.done {
            return None;
        }
        let start = self.window_start?;
        if now_us.saturating_sub(start) < self.window_us {
            return None;
        }
        self.done = true;
        self.classify()
    }

    fn classify(&self) -> Option<LearnCapture> {
        // 14-bit pair: MSB below 32 with its LSB (+32) in the same window.
        for (&(channel, cc), msb_stats) in &self.ccs {
            if cc < 32
                && self.hint != LearnHint::Relative(RelativeEncoding::Offset64)
                && let Some(lsb_stats) = self.ccs.get(&(channel, cc + 32))
                && matches!(self.hint, LearnHint::Auto | LearnHint::Absolute)
            {
                return Some(LearnCapture {
                    midi: MidiInput::Cc14 {
                        channel: channel + 1,
                        msb: cc,
                        lsb: cc + 32,
                    },
                    value: ValueSpec::Absolute { invert: false },
                    messages: msb_stats.values.len() + lsb_stats.values.len(),
                    description: self.first_description.clone().unwrap_or_default(),
                });
            }
        }
        // Otherwise the busiest CC wins (earliest on ties).
        let (&(channel, cc), stats) = self
            .ccs
            .iter()
            .max_by_key(|(_, s)| (s.values.len(), std::cmp::Reverse(s.first_ts)))?;
        let value = match self.hint {
            LearnHint::Absolute => ValueSpec::Absolute { invert: false },
            LearnHint::Relative(encoding) => ValueSpec::Relative {
                encoding,
                scale: 1.0,
            },
            LearnHint::Button => ValueSpec::Button {
                threshold: None,
                edge: Edge::Both,
                invert: false,
            },
            LearnHint::Auto => guess_value(&stats.values),
        };
        Some(LearnCapture {
            midi: MidiInput::Cc {
                channel: channel + 1,
                number: cc,
            },
            value,
            messages: stats.values.len(),
            description: self.first_description.clone().unwrap_or_default(),
        })
    }
}

fn guess_value(values: &[u8]) -> ValueSpec {
    let twos = values.iter().all(|v| matches!(v, 1..=3 | 125..=127));
    let offset = values.iter().all(|v| (61..=67).contains(v) && *v != 64);
    if values.len() >= 2 && twos {
        ValueSpec::Relative {
            encoding: RelativeEncoding::TwosComplement,
            scale: 1.0,
        }
    } else if values.len() >= 2 && offset {
        ValueSpec::Relative {
            encoding: RelativeEncoding::Offset64,
            scale: 1.0,
        }
    } else {
        ValueSpec::Absolute { invert: false }
    }
}

/// Builds a control entry from a capture.
pub fn build_control(
    capture: &LearnCapture,
    id: &str,
    label: Option<&str>,
    action: ControlAction,
) -> Control {
    Control {
        id: id.to_owned(),
        source_ref: None,
        label: label.map(str::to_owned),
        midi: capture.midi,
        when: BTreeMap::new(),
        action,
        value: capture.value,
        verify: None,
    }
}

impl Mapping {
    /// Adds a control, or replaces the one with the same id. The mapping is
    /// left unchanged if the result is invalid (for example when the learned
    /// input is already used by another control).
    pub fn upsert_control(&mut self, control: Control) -> Result<(), MappingError> {
        let previous = self.controls.clone();
        match self.controls.iter_mut().find(|c| c.id == control.id) {
            Some(slot) => *slot = control,
            None => self.controls.push(control),
        }
        if let Err(e) = self.validate() {
            self.controls = previous;
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendeck_contracts::ids::DeckId;

    fn session() -> LearnSession {
        LearnSession::with_window(LearnHint::Auto, 100_000)
    }

    #[test]
    fn note_is_a_button_at_once() {
        let mut s = session();
        let c = s.feed(0, &[0x91, 0x0B, 0x7F]).expect("capture");
        assert_eq!(
            c.midi,
            MidiInput::Note {
                channel: 2,
                number: 0x0B
            }
        );
        assert!(matches!(c.value, ValueSpec::Button { .. }));
        assert!(s.is_done());
        assert!(s.feed(1, &[0x90, 1, 1]).is_none());
    }

    #[test]
    fn note_off_alone_is_ignored() {
        let mut s = session();
        assert!(s.feed(0, &[0x80, 0x0B, 0]).is_none());
        assert!(!s.is_done());
    }

    #[test]
    fn fourteen_bit_pair_is_detected() {
        let mut s = session();
        assert!(s.feed(0, &[0xB0, 0x00, 0x40]).is_none());
        assert!(s.feed(100, &[0xB0, 0x20, 0x11]).is_none());
        assert_eq!(s.deadline_us(), Some(100_000));
        let c = s.tick(100_000).expect("window over");
        assert_eq!(
            c.midi,
            MidiInput::Cc14 {
                channel: 1,
                msb: 0,
                lsb: 32
            }
        );
        assert!(matches!(c.value, ValueSpec::Absolute { .. }));
    }

    #[test]
    fn relative_encodings_are_guessed() {
        let mut s = session();
        s.feed(0, &[0xB6, 0x40, 0x01]);
        s.feed(10, &[0xB6, 0x40, 0x7F]);
        s.feed(20, &[0xB6, 0x40, 0x01]);
        let c = s.tick(200_000).expect("capture");
        assert_eq!(
            c.value,
            ValueSpec::Relative {
                encoding: RelativeEncoding::TwosComplement,
                scale: 1.0
            }
        );
        assert_eq!(c.messages, 3);

        let mut s = session();
        s.feed(0, &[0xB0, 0x22, 0x41]);
        s.feed(10, &[0xB0, 0x22, 0x3F]);
        let c = s.tick(200_000).expect("capture");
        assert!(matches!(
            c.value,
            ValueSpec::Relative {
                encoding: RelativeEncoding::Offset64,
                ..
            }
        ));
    }

    #[test]
    fn single_cc_is_absolute_and_hint_wins() {
        let mut s = session();
        s.feed(0, &[0xB6, 0x17, 0x01]);
        let c = s.tick(200_000).expect("capture");
        assert!(matches!(c.value, ValueSpec::Absolute { .. }));

        let mut s = LearnSession::with_window(
            LearnHint::Relative(RelativeEncoding::SignMagnitude),
            100_000,
        );
        s.feed(0, &[0xB6, 0x17, 0x01]);
        let c = s.tick(200_000).expect("capture");
        assert!(matches!(
            c.value,
            ValueSpec::Relative {
                encoding: RelativeEncoding::SignMagnitude,
                ..
            }
        ));

        let mut s = LearnSession::new(LearnHint::Button);
        let c = s.feed(0, &[0xB0, 0x10, 0x7F]).expect("immediate");
        assert!(matches!(c.value, ValueSpec::Button { .. }));
    }

    #[test]
    fn late_message_finishes_the_window_first() {
        let mut s = session();
        s.feed(0, &[0xB0, 0x07, 0x10]);
        let c = s.feed(500_000, &[0xB0, 0x0B, 0x10]).expect("capture");
        assert_eq!(
            c.midi,
            MidiInput::Cc {
                channel: 1,
                number: 7
            }
        );
    }

    #[test]
    fn builds_and_upserts_a_control() {
        let mut mapping = crate::flx4_mapping().expect("flx4");
        let mut s = session();
        let c = s.feed(0, &[0x92, 0x05, 0x7F]).expect("capture");
        let control = build_control(
            &c,
            "user.deck1.play",
            Some("learned"),
            ControlAction::PlayPause {
                deck: DeckId::new(0).expect("deck"),
            },
        );
        mapping.upsert_control(control.clone()).expect("free input");
        assert!(mapping.controls.iter().any(|c| c.id == "user.deck1.play"));

        // The same input again under another id is refused and nothing changes.
        let before = mapping.controls.len();
        let mut clash = control;
        clash.id = "user.other".into();
        assert!(mapping.upsert_control(clash).is_err());
        assert_eq!(mapping.controls.len(), before);
    }
}
