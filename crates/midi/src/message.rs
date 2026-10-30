//! Raw MIDI message parsing and description.
//!
//! Only channel messages matter for mappings. A note-on with velocity 0 is
//! normalised to a note-off, as the MIDI specification defines it. Channels
//! here are the 0-based nibble; mapping files and the monitor text use 1-16.

/// A decoded channel voice message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMessage {
    /// Note off (also a note-on with velocity 0).
    NoteOff {
        /// Channel nibble, 0-15.
        channel: u8,
        /// Note number.
        note: u8,
        /// Release velocity.
        velocity: u8,
    },
    /// Note on with velocity above 0.
    NoteOn {
        /// Channel nibble, 0-15.
        channel: u8,
        /// Note number.
        note: u8,
        /// Velocity, 1-127.
        velocity: u8,
    },
    /// Control change.
    ControlChange {
        /// Channel nibble, 0-15.
        channel: u8,
        /// Controller number.
        controller: u8,
        /// Value, 0-127.
        value: u8,
    },
    /// Pitch bend, 14-bit, centre 0x2000. Parsed and shown by the monitor;
    /// the mapping format has no pitch-bend input because no supported
    /// controller uses it.
    PitchBend {
        /// Channel nibble, 0-15.
        channel: u8,
        /// 0..=16383.
        value: u16,
    },
    /// Program change.
    ProgramChange {
        /// Channel nibble, 0-15.
        channel: u8,
        /// Program, 0-127.
        program: u8,
    },
}

impl MidiMessage {
    /// Parses one complete message. Returns `None` for system messages,
    /// running-status fragments, truncated messages and aftertouch.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let status = *bytes.first()?;
        if !(0x80..0xF0).contains(&status) {
            return None;
        }
        let channel = status & 0x0F;
        let d1 = bytes.get(1).copied();
        let d2 = bytes.get(2).copied();
        let data = |b: Option<u8>| b.filter(|v| *v < 0x80);
        match status & 0xF0 {
            0x80 => Some(Self::NoteOff {
                channel,
                note: data(d1)?,
                velocity: data(d2)?,
            }),
            0x90 => {
                let note = data(d1)?;
                let velocity = data(d2)?;
                Some(if velocity == 0 {
                    Self::NoteOff {
                        channel,
                        note,
                        velocity,
                    }
                } else {
                    Self::NoteOn {
                        channel,
                        note,
                        velocity,
                    }
                })
            }
            0xB0 => Some(Self::ControlChange {
                channel,
                controller: data(d1)?,
                value: data(d2)?,
            }),
            0xC0 => Some(Self::ProgramChange {
                channel,
                program: data(d1)?,
            }),
            0xE0 => Some(Self::PitchBend {
                channel,
                value: u16::from(data(d1)?) | (u16::from(data(d2)?) << 7),
            }),
            _ => None,
        }
    }

    /// Channel nibble, 0-15.
    pub fn channel(self) -> u8 {
        match self {
            Self::NoteOff { channel, .. }
            | Self::NoteOn { channel, .. }
            | Self::ControlChange { channel, .. }
            | Self::PitchBend { channel, .. }
            | Self::ProgramChange { channel, .. } => channel,
        }
    }
}

/// Human-readable text for the monitor, e.g. `Note On ch1 0x0B vel 127`.
pub fn describe(bytes: &[u8]) -> String {
    let hex = |b: &[u8]| {
        b.iter()
            .map(|v| format!("{v:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // Show the original status: a velocity-0 note-on is still "Note On".
    let raw_note_on = bytes.first().is_some_and(|s| s & 0xF0 == 0x90);
    match MidiMessage::parse(bytes) {
        Some(MidiMessage::NoteOn {
            channel,
            note,
            velocity,
        }) => format!("Note On ch{} 0x{note:02X} vel {velocity}", channel + 1),
        Some(MidiMessage::NoteOff {
            channel,
            note,
            velocity,
        }) => {
            let name = if raw_note_on { "Note On" } else { "Note Off" };
            format!("{name} ch{} 0x{note:02X} vel {velocity}", channel + 1)
        }
        Some(MidiMessage::ControlChange {
            channel,
            controller,
            value,
        }) => format!("CC ch{} 0x{controller:02X} = {value}", channel + 1),
        Some(MidiMessage::PitchBend { channel, value }) => {
            format!("Pitch bend ch{} {value}", channel + 1)
        }
        Some(MidiMessage::ProgramChange { channel, program }) => {
            format!("Program ch{} {program}", channel + 1)
        }
        None => format!("Raw {}", hex(bytes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_velocity_zero_is_note_off() {
        assert_eq!(
            MidiMessage::parse(&[0x90, 0x0B, 0x00]),
            Some(MidiMessage::NoteOff {
                channel: 0,
                note: 0x0B,
                velocity: 0
            })
        );
        assert_eq!(
            MidiMessage::parse(&[0x91, 0x0B, 0x7F]),
            Some(MidiMessage::NoteOn {
                channel: 1,
                note: 0x0B,
                velocity: 0x7F
            })
        );
    }

    #[test]
    fn cc_and_pitch_bend() {
        assert_eq!(
            MidiMessage::parse(&[0xB6, 0x1F, 0x40]),
            Some(MidiMessage::ControlChange {
                channel: 6,
                controller: 0x1F,
                value: 0x40
            })
        );
        assert_eq!(
            MidiMessage::parse(&[0xE0, 0x00, 0x40]),
            Some(MidiMessage::PitchBend {
                channel: 0,
                value: 0x2000
            })
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(MidiMessage::parse(&[]), None);
        assert_eq!(MidiMessage::parse(&[0x40, 0x01]), None);
        assert_eq!(MidiMessage::parse(&[0x90, 0x0B]), None);
        assert_eq!(MidiMessage::parse(&[0x90, 0x8B, 0x01]), None);
        assert_eq!(MidiMessage::parse(&[0xF8]), None);
        assert_eq!(MidiMessage::parse(&[0xF0, 0x01, 0xF7]), None);
    }

    #[test]
    fn describe_text() {
        assert_eq!(describe(&[0x90, 0x0B, 0x7F]), "Note On ch1 0x0B vel 127");
        assert_eq!(describe(&[0x90, 0x0B, 0x00]), "Note On ch1 0x0B vel 0");
        assert_eq!(describe(&[0x80, 0x0B, 0x00]), "Note Off ch1 0x0B vel 0");
        assert_eq!(describe(&[0xB1, 0x02, 0x10]), "CC ch2 0x02 = 16");
        assert_eq!(describe(&[0xF0, 0x7E, 0xF7]), "Raw F0 7E F7");
    }
}
