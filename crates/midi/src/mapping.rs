//! Mapping file types, loading and validation (ADR 0007).
//!
//! A mapping is plain data: MIDI inputs to [`ControlAction`]s, and
//! [`FeedbackTarget`]s to MIDI outputs. Channels are written 1-16 in the file
//! (as printed in the controller's message list) and stay 1-based in these
//! types; the engines subtract one.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use opendeck_contracts::control::{ControlAction, FeedbackTarget};
use serde::{Deserialize, Serialize};

use crate::MAPPING_FORMAT_VERSION;
use crate::error::MappingError;

/// A complete controller mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    /// Schema reference, informational.
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// File format version, [`MAPPING_FORMAT_VERSION`].
    pub format_version: u32,
    /// Stable id; DB overrides reference it.
    pub id: String,
    /// Mapping revision.
    pub version: u32,
    /// Display name.
    pub name: String,
    /// Document the bytes come from.
    pub source: String,
    /// Port name patterns.
    pub devices: Devices,
    /// Held buttons that switch layers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<Modifier>,
    /// Input controls.
    pub controls: Vec<Control>,
    /// LED and meter outputs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feedback: Vec<Feedback>,
    /// Raw messages sent when the output opens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_connect: Vec<RawMessage>,
    /// Source rows that are deliberately not mapped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmapped: Vec<Unmapped>,
}

/// Case-insensitive substrings matched against port names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Devices {
    /// Input port patterns.
    pub input: Vec<String>,
    /// Output port patterns (LED feedback).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<String>,
}

/// Kind of a note or CC address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// Note on/off.
    Note,
    /// Control change.
    Cc,
}

/// A single note or CC address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidiAddress {
    /// Note or CC.
    #[serde(rename = "type")]
    pub kind: MessageKind,
    /// Channel 1-16.
    pub channel: u8,
    /// Note or controller number.
    pub number: u8,
}

/// A MIDI input of a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum MidiInput {
    /// A note.
    Note {
        /// Channel 1-16.
        channel: u8,
        /// Note number.
        number: u8,
    },
    /// A 7-bit control change.
    Cc {
        /// Channel 1-16.
        channel: u8,
        /// Controller number.
        number: u8,
    },
    /// A 14-bit value from an MSB and an LSB control change.
    Cc14 {
        /// Channel 1-16.
        channel: u8,
        /// MSB controller number.
        msb: u8,
        /// LSB controller number.
        lsb: u8,
    },
}

impl MidiInput {
    /// Channel 1-16.
    pub fn channel(&self) -> u8 {
        match *self {
            Self::Note { channel, .. } | Self::Cc { channel, .. } | Self::Cc14 { channel, .. } => {
                channel
            }
        }
    }
}

/// A held button that switches a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    /// Id referenced by `when` maps.
    pub id: String,
    /// Source row.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    /// The button.
    pub midi: MidiAddress,
    /// Unconfirmed value note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
}

/// Which button edges produce events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    /// Press and release.
    #[default]
    Both,
    /// Press only.
    Press,
    /// Release only.
    Release,
}

impl Edge {
    fn is_both(&self) -> bool {
        *self == Edge::Both
    }
}

/// Encoding of a relative encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelativeEncoding {
    /// `delta = v - 64` (FLX4 jog).
    Offset64,
    /// `delta = v` below 64, `v - 128` otherwise (FLX4 browse).
    TwosComplement,
    /// Bit 6 is the sign, bits 0-5 the magnitude.
    SignMagnitude,
}

impl RelativeEncoding {
    /// Signed tick count of a raw 7-bit value.
    pub fn delta(self, v: u8) -> i16 {
        let v = i16::from(v & 0x7F);
        match self {
            Self::Offset64 => v - 64,
            Self::TwosComplement => {
                if v < 64 {
                    v
                } else {
                    v - 128
                }
            }
            Self::SignMagnitude => {
                let magnitude = v & 0x3F;
                if v & 0x40 != 0 { -magnitude } else { magnitude }
            }
        }
    }
}

fn one() -> f32 {
    1.0
}

fn is_false(v: &bool) -> bool {
    !*v
}

fn is_one(v: &f32) -> bool {
    (*v - 1.0).abs() < f32::EPSILON
}

/// How a control's data byte becomes a [`ControlValue`](opendeck_contracts::control::ControlValue).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ValueSpec {
    /// Press when data2 reaches the threshold, release otherwise.
    Button {
        /// Press threshold, default 1.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        threshold: Option<u8>,
        /// Edges that produce events.
        #[serde(default, skip_serializing_if = "Edge::is_both")]
        edge: Edge,
        /// Report press as `false` and release as `true`.
        #[serde(default, skip_serializing_if = "is_false")]
        invert: bool,
    },
    /// 7-bit or 14-bit position scaled to `[0, 1]`.
    Absolute {
        /// Report `1 - x`.
        #[serde(default, skip_serializing_if = "is_false")]
        invert: bool,
    },
    /// Relative encoder, output is `delta * scale`.
    Relative {
        /// Encoding of the data byte.
        encoding: RelativeEncoding,
        /// Units per tick.
        #[serde(default = "one", skip_serializing_if = "is_one")]
        scale: f32,
    },
}

/// An input control.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    /// Unique id, e.g. `deck1.play`.
    pub id: String,
    /// Source row.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    /// UI name from the source document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// MIDI input.
    pub midi: MidiInput,
    /// Modifier states required to fire.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, bool>,
    /// What it does.
    pub action: ControlAction,
    /// How the value is decoded.
    pub value: ValueSpec,
    /// Unconfirmed value note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
}

/// Blink behaviour of a light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Blink {
    /// Software toggling every `period_ms`.
    Timer {
        /// Toggle period in milliseconds, at least 50.
        period_ms: u32,
    },
    /// The device blinks when it receives `blink_value`.
    Device {
        /// Data byte that starts a hardware blink.
        blink_value: u8,
    },
}

/// Encoding of a feedback output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Output {
    /// On/off light.
    Light {
        /// Data byte for on.
        on: u8,
        /// Data byte for off.
        off: u8,
        /// Blink behaviour, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blink: Option<Blink>,
    },
    /// Level, linear from `min` to `max`.
    Level {
        /// Data byte at level 0.
        min: u8,
        /// Data byte at level 1.
        max: u8,
    },
}

/// A feedback entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feedback {
    /// Source row.
    #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    /// Label from the source document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Modifier states required for the output to be sent.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub when: BTreeMap<String, bool>,
    /// Application state.
    pub target: FeedbackTarget,
    /// Output address.
    pub midi: MidiAddress,
    /// Value encoding.
    pub output: Output,
    /// Unconfirmed value note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
}

/// A raw message sent when the output opens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMessage {
    /// Status byte and data, or a complete SysEx.
    pub bytes: Vec<u8>,
    /// Free comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Unconfirmed value note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
}

/// A source row that is deliberately not mapped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unmapped {
    /// Source row.
    #[serde(rename = "ref")]
    pub source_ref: String,
    /// Label from the source document.
    pub label: String,
    /// MIDI input of the row.
    pub midi: MidiInput,
    /// Why it has no action.
    pub reason: String,
}

/// One `verify` marker in a mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyItem {
    /// What carries it: `modifier`, `control`, `feedback` or `on_connect`.
    pub kind: &'static str,
    /// Control or modifier id, or the index for feedback and raw messages.
    pub id: String,
    /// The note, which starts with a question id such as `Q07`.
    pub text: String,
}

impl Mapping {
    /// Parses and validates a mapping from JSON text.
    pub fn from_json(text: &str) -> Result<Self, MappingError> {
        let mapping: Mapping = serde_json::from_str(text)?;
        mapping.validate()?;
        Ok(mapping)
    }

    /// Reads, parses and validates a mapping file.
    pub fn from_path(path: &Path) -> Result<Self, MappingError> {
        let text = std::fs::read_to_string(path).map_err(|source| MappingError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_json(&text)
    }

    /// Serializes to pretty JSON (used by learn to export a mapping).
    pub fn to_json(&self) -> Result<String, MappingError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Every `verify` marker, in file order.
    pub fn verify_items(&self) -> Vec<VerifyItem> {
        let mut items = Vec::new();
        for m in &self.modifiers {
            if let Some(text) = &m.verify {
                items.push(VerifyItem {
                    kind: "modifier",
                    id: m.id.clone(),
                    text: text.clone(),
                });
            }
        }
        for c in &self.controls {
            if let Some(text) = &c.verify {
                items.push(VerifyItem {
                    kind: "control",
                    id: c.id.clone(),
                    text: text.clone(),
                });
            }
        }
        for (i, f) in self.feedback.iter().enumerate() {
            if let Some(text) = &f.verify {
                items.push(VerifyItem {
                    kind: "feedback",
                    id: i.to_string(),
                    text: text.clone(),
                });
            }
        }
        for (i, r) in self.on_connect.iter().enumerate() {
            if let Some(text) = &r.verify {
                items.push(VerifyItem {
                    kind: "on_connect",
                    id: i.to_string(),
                    text: text.clone(),
                });
            }
        }
        items
    }

    /// Number of `verify` markers.
    pub fn verify_count(&self) -> usize {
        self.verify_items().len()
    }

    /// Replaces the MIDI input of controls by id (user overrides from the
    /// `midi_mapping_overrides` table, ADR 0005). Unknown ids are returned so
    /// the caller can flag them. The result is validated again.
    pub fn apply_input_overrides(
        &mut self,
        overrides: &[(String, MidiInput)],
    ) -> Result<Vec<String>, MappingError> {
        let mut unknown = Vec::new();
        for (id, midi) in overrides {
            match self.controls.iter_mut().find(|c| &c.id == id) {
                Some(control) => control.midi = *midi,
                None => unknown.push(id.clone()),
            }
        }
        self.validate()?;
        Ok(unknown)
    }

    /// Checks the rules of ADR 0007 that serde cannot express.
    pub fn validate(&self) -> Result<(), MappingError> {
        if self.format_version != MAPPING_FORMAT_VERSION {
            return Err(MappingError::invalid(
                "format_version",
                format!(
                    "expected {MAPPING_FORMAT_VERSION}, found {}",
                    self.format_version
                ),
            ));
        }
        if !is_slug(&self.id) {
            return Err(MappingError::invalid(
                "id",
                "must be lowercase words joined by '-'",
            ));
        }
        if self.version == 0 {
            return Err(MappingError::invalid("version", "must be at least 1"));
        }
        if self.devices.input.is_empty() {
            return Err(MappingError::invalid(
                "devices.input",
                "needs at least one pattern",
            ));
        }

        let mut modifier_ids = HashSet::new();
        // Input addresses in use: (is_cc, channel, number) -> owner and `when` signature.
        let mut inputs: Claims = HashMap::new();
        for (i, m) in self.modifiers.iter().enumerate() {
            let at = format!("modifiers[{i}] ({})", m.id);
            check_ident(&at, &m.id)?;
            if !modifier_ids.insert(m.id.as_str()) {
                return Err(MappingError::invalid(at, "duplicate modifier id"));
            }
            check_address(&at, &m.midi)?;
            claim(
                &mut inputs,
                &at,
                (
                    m.midi.kind == MessageKind::Cc,
                    m.midi.channel,
                    m.midi.number,
                ),
                &m.id,
                &BTreeMap::new(),
            )?;
        }

        let mut control_ids = HashSet::new();
        for (i, c) in self.controls.iter().enumerate() {
            let at = format!("controls[{i}] ({})", c.id);
            check_ident(&at, &c.id)?;
            if !control_ids.insert(c.id.as_str()) {
                return Err(MappingError::invalid(at, "duplicate control id"));
            }
            check_when(&at, &c.when, &modifier_ids)?;
            match (&c.midi, &c.value) {
                (MidiInput::Note { channel, number }, value) => {
                    check_channel_number(&at, *channel, *number)?;
                    if matches!(value, ValueSpec::Relative { .. }) {
                        return Err(MappingError::invalid(at, "relative needs a cc input"));
                    }
                    claim(&mut inputs, &at, (false, *channel, *number), &c.id, &c.when)?;
                }
                (MidiInput::Cc { channel, number }, _) => {
                    check_channel_number(&at, *channel, *number)?;
                    claim(&mut inputs, &at, (true, *channel, *number), &c.id, &c.when)?;
                }
                (MidiInput::Cc14 { channel, msb, lsb }, value) => {
                    check_channel_number(&at, *channel, *msb)?;
                    check_channel_number(&at, *channel, *lsb)?;
                    if msb == lsb {
                        return Err(MappingError::invalid(at, "msb and lsb must differ"));
                    }
                    if !matches!(value, ValueSpec::Absolute { .. }) {
                        return Err(MappingError::invalid(at, "cc14 needs an absolute value"));
                    }
                    claim(&mut inputs, &at, (true, *channel, *msb), &c.id, &c.when)?;
                    claim(&mut inputs, &at, (true, *channel, *lsb), &c.id, &c.when)?;
                }
            }
            if let ValueSpec::Relative { scale, .. } = c.value
                && (!scale.is_finite() || scale == 0.0)
            {
                return Err(MappingError::invalid(
                    at,
                    "scale must be finite and non-zero",
                ));
            }
            if let ValueSpec::Button {
                threshold: Some(t), ..
            } = c.value
                && t > 127
            {
                return Err(MappingError::invalid(at, "threshold above 127"));
            }
        }

        for (i, f) in self.feedback.iter().enumerate() {
            let at = format!("feedback[{i}] ({:?})", f.target);
            check_address(&at, &f.midi)?;
            check_when(&at, &f.when, &modifier_ids)?;
            match f.output {
                Output::Light { on, off, blink } => {
                    if on > 127 || off > 127 {
                        return Err(MappingError::invalid(at, "light value above 127"));
                    }
                    match blink {
                        Some(Blink::Timer { period_ms }) if period_ms < 50 => {
                            return Err(MappingError::invalid(at, "blink period below 50 ms"));
                        }
                        Some(Blink::Device { blink_value }) if blink_value > 127 => {
                            return Err(MappingError::invalid(at, "blink value above 127"));
                        }
                        _ => {}
                    }
                }
                Output::Level { min, max } => {
                    if min > 127 || max > 127 {
                        return Err(MappingError::invalid(at, "level value above 127"));
                    }
                }
            }
        }

        for (i, r) in self.on_connect.iter().enumerate() {
            check_raw(&format!("on_connect[{i}]"), &r.bytes)?;
        }
        Ok(())
    }
}

fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|w| {
            !w.is_empty()
                && w.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

fn check_ident(at: &str, id: &str) -> Result<(), MappingError> {
    let ok = !id.is_empty()
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
    if ok {
        Ok(())
    } else {
        Err(MappingError::invalid(
            at,
            format!("id {id:?} must be dot-separated lowercase words"),
        ))
    }
}

fn check_channel_number(at: &str, channel: u8, number: u8) -> Result<(), MappingError> {
    if !(1..=16).contains(&channel) {
        return Err(MappingError::invalid(
            at,
            format!("channel {channel} outside 1-16"),
        ));
    }
    if number > 127 {
        return Err(MappingError::invalid(
            at,
            format!("number {number} above 127"),
        ));
    }
    Ok(())
}

fn check_address(at: &str, a: &MidiAddress) -> Result<(), MappingError> {
    check_channel_number(at, a.channel, a.number)
}

fn check_when(
    at: &str,
    when: &BTreeMap<String, bool>,
    modifiers: &HashSet<&str>,
) -> Result<(), MappingError> {
    for key in when.keys() {
        if !modifiers.contains(key.as_str()) {
            return Err(MappingError::invalid(
                at,
                format!("when refers to unknown modifier {key:?}"),
            ));
        }
    }
    Ok(())
}

/// Input addresses in use: `(is_cc, channel, number)` to owners with their `when` signature.
type Claims = HashMap<(bool, u8, u8), Vec<(String, Vec<(String, bool)>)>>;

fn claim(
    inputs: &mut Claims,
    at: &str,
    key: (bool, u8, u8),
    id: &str,
    when: &BTreeMap<String, bool>,
) -> Result<(), MappingError> {
    let signature: Vec<(String, bool)> = when.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let owners = inputs.entry(key).or_default();
    if let Some((other, _)) = owners.iter().find(|(_, sig)| *sig == signature) {
        return Err(MappingError::invalid(
            at,
            format!(
                "MIDI input {} ch{} {} is already used by {other}",
                if key.0 { "cc" } else { "note" },
                key.1,
                key.2
            ),
        ));
    }
    owners.push((id.to_owned(), signature));
    Ok(())
}

/// Validates one raw message: channel message (status + data bytes) or a
/// complete SysEx.
pub fn check_raw(at: &str, bytes: &[u8]) -> Result<(), MappingError> {
    let Some(&status) = bytes.first() else {
        return Err(MappingError::invalid(at, "empty message"));
    };
    if status < 0x80 {
        return Err(MappingError::invalid(
            at,
            "first byte must be a status byte",
        ));
    }
    if status == 0xF0 {
        if bytes.len() < 2 || bytes.last() != Some(&0xF7) {
            return Err(MappingError::invalid(at, "SysEx must end with F7"));
        }
        if bytes[1..bytes.len() - 1].iter().any(|b| *b > 0x7F) {
            return Err(MappingError::invalid(at, "SysEx data bytes must be 7-bit"));
        }
        return Ok(());
    }
    if bytes.len() > 3 {
        return Err(MappingError::invalid(
            at,
            "only SysEx may be longer than 3 bytes",
        ));
    }
    if bytes[1..].iter().any(|b| *b > 0x7F) {
        return Err(MappingError::invalid(at, "data bytes must be 7-bit"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{
        "format_version": 1, "id": "test-pad", "version": 1, "name": "T", "source": "t",
        "devices": { "input": ["T"] },
        "modifiers": [{ "id": "shift", "midi": { "type": "note", "channel": 1, "number": 63 } }],
        "controls": [
            { "id": "deck1.play", "midi": { "type": "note", "channel": 1, "number": 11 },
              "action": { "action": "play_pause", "deck": 0 }, "value": { "kind": "button" } }
        ]
    }"#;

    fn with(edit: impl FnOnce(&mut Mapping)) -> Result<Mapping, MappingError> {
        let mut m = Mapping::from_json(BASE).expect("base is valid");
        edit(&mut m);
        m.validate().map(|()| m)
    }

    #[test]
    fn base_loads() {
        let m = Mapping::from_json(BASE).expect("loads");
        assert_eq!(m.controls.len(), 1);
        assert_eq!(m.verify_count(), 0);
    }

    #[test]
    fn relative_decoders() {
        assert_eq!(RelativeEncoding::Offset64.delta(0x41), 1);
        assert_eq!(RelativeEncoding::Offset64.delta(0x3F), -1);
        assert_eq!(RelativeEncoding::Offset64.delta(0x40), 0);
        assert_eq!(RelativeEncoding::TwosComplement.delta(0x01), 1);
        assert_eq!(RelativeEncoding::TwosComplement.delta(0x7F), -1);
        assert_eq!(RelativeEncoding::TwosComplement.delta(0x3F), 63);
        assert_eq!(RelativeEncoding::TwosComplement.delta(0x40), -64);
        assert_eq!(RelativeEncoding::SignMagnitude.delta(0x03), 3);
        assert_eq!(RelativeEncoding::SignMagnitude.delta(0x43), -3);
    }

    #[test]
    fn duplicate_ids_and_inputs_are_rejected() {
        let dup_id = with(|m| {
            let mut c = m.controls[0].clone();
            c.midi = MidiInput::Note {
                channel: 1,
                number: 12,
            };
            m.controls.push(c);
        });
        assert!(dup_id.is_err());
        let dup_input = with(|m| {
            let mut c = m.controls[0].clone();
            c.id = "deck1.other".into();
            m.controls.push(c);
        });
        assert!(dup_input.is_err());
        let modifier_clash = with(|m| {
            m.controls[0].midi = MidiInput::Note {
                channel: 1,
                number: 63,
            };
        });
        assert!(modifier_clash.is_err());
    }

    #[test]
    fn same_input_with_different_when_is_allowed() {
        let ok = with(|m| {
            let mut c = m.controls[0].clone();
            c.id = "deck1.play.shift".into();
            c.when.insert("shift".into(), true);
            m.controls.push(c);
        });
        assert!(ok.is_ok());
    }

    #[test]
    fn when_must_name_a_modifier() {
        let bad = with(|m| {
            m.controls[0].when.insert("nope".into(), true);
        });
        assert!(bad.is_err());
    }

    #[test]
    fn cc14_lsb_collision_is_rejected() {
        let bad = with(|m| {
            m.controls[0].midi = MidiInput::Cc14 {
                channel: 1,
                msb: 0,
                lsb: 32,
            };
            m.controls[0].value = ValueSpec::Absolute { invert: false };
            let mut c = m.controls[0].clone();
            c.id = "other".into();
            c.midi = MidiInput::Cc {
                channel: 1,
                number: 32,
            };
            m.controls.push(c);
        });
        assert!(bad.is_err());
    }

    #[test]
    fn raw_messages_allow_sysex() {
        assert!(check_raw("x", &[0x90, 0x17, 0x7F]).is_ok());
        assert!(check_raw("x", &[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]).is_ok());
        assert!(check_raw("x", &[0xF0, 0x7E, 0x80, 0xF7]).is_err());
        assert!(check_raw("x", &[0xF0, 0x7E]).is_err());
        assert!(check_raw("x", &[0x90, 0x17, 0x7F, 0x00]).is_err());
        assert!(check_raw("x", &[0x17, 0x7F]).is_err());
        assert!(check_raw("x", &[]).is_err());
    }

    #[test]
    fn on_connect_sysex_loads() {
        let text = BASE.replace(
            r#""controls": ["#,
            r#""on_connect": [{ "bytes": [240, 126, 127, 6, 1, 247], "comment": "identity request" }],
            "controls": ["#,
        );
        let m = Mapping::from_json(&text).expect("sysex on_connect loads");
        assert_eq!(m.on_connect[0].bytes.len(), 6);
    }

    #[test]
    fn overrides_replace_midi_and_report_unknown() {
        let mut m = Mapping::from_json(BASE).expect("loads");
        let unknown = m
            .apply_input_overrides(&[
                (
                    "deck1.play".into(),
                    MidiInput::Note {
                        channel: 3,
                        number: 1,
                    },
                ),
                (
                    "gone".into(),
                    MidiInput::Note {
                        channel: 3,
                        number: 2,
                    },
                ),
            ])
            .expect("valid after override");
        assert_eq!(unknown, vec!["gone".to_owned()]);
        assert_eq!(m.controls[0].midi.channel(), 3);
    }
}
