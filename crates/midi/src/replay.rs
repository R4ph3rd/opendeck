//! Replay harness: recorded MIDI in, expected events and LED messages out.
//!
//! A fixture is JSONL: one JSON object per line, executed in order against a
//! [`Pipeline`]. Times are microseconds and must not decrease; a line without
//! `t` runs at the time of the previous line.
//!
//! | Line | Effect |
//! |---|---|
//! | `{"t": 1000, "in": [144, 11, 127]}` | feed a raw message to the input engine |
//! | `{"t": 6000, "tick": true}` | run the engine timers (14-bit MSB timeout, blink, rate limit) |
//! | `{"t": 7000, "feedback": {"target": {...}, "value": {...}}}` | set an application state; the messages that are due are collected |
//! | `{"expect": [{"action": {...}, "value": {...}}]}` | the events produced since the last `expect` (optional `timestamp_us` is checked when present) |
//! | `{"expect_out": [[144, 11, 127]]}` | the messages produced since the last `expect_out` |
//! | `{"expect_modifiers": {"shift_deck1": true}}` | modifier states |
//! | `{"note": "text"}` | ignored, documents the fixture |
//!
//! `action`, `target` and `value` use the exact serde forms of the contracts
//! types. Numbers compare with a tolerance of 1e-4. Events or messages left
//! over at the end of a fixture fail it, so a fixture also proves what did
//! **not** happen.

use opendeck_contracts::control::{ControlEvent, FeedbackTarget, FeedbackValue};
use serde_json::Value;

use crate::error::MappingError;
use crate::feedback::{FeedbackConfig, OutMessage};
use crate::mapping::Mapping;
use crate::pipeline::Pipeline;

/// A fixture failed.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// The mapping did not load.
    #[error("mapping: {0}")]
    Mapping(#[from] MappingError),
    /// A line is malformed or an expectation failed.
    #[error("fixture line {line}: {message}")]
    Fixture {
        /// 1-based line number.
        line: usize,
        /// What went wrong.
        message: String,
    },
}

/// Summary of a passing fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplayReport {
    /// Lines executed (blank lines and notes excluded).
    pub steps: usize,
    /// Expectations that passed.
    pub checks: usize,
    /// Raw messages fed in.
    pub messages_in: usize,
}

fn fail<T>(line: usize, message: impl Into<String>) -> Result<T, ReplayError> {
    Err(ReplayError::Fixture {
        line,
        message: message.into(),
    })
}

/// Runs a fixture against a mapping with the default feedback policy.
pub fn run(mapping: &Mapping, fixture: &str) -> Result<ReplayReport, ReplayError> {
    run_with(mapping, FeedbackConfig::default(), fixture)
}

/// Runs a fixture with an explicit feedback policy.
pub fn run_with(
    mapping: &Mapping,
    config: FeedbackConfig,
    fixture: &str,
) -> Result<ReplayReport, ReplayError> {
    let mut pipeline = Pipeline::new(mapping.clone(), config)?;
    let mut report = ReplayReport::default();
    let mut events: Vec<ControlEvent> = Vec::new();
    let mut out: Vec<OutMessage> = Vec::new();
    let mut now: u64 = 0;
    let mut last_line = 0;

    for (idx, text) in fixture.lines().enumerate() {
        let line = idx + 1;
        last_line = line;
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => return fail(line, format!("not JSON: {e}")),
        };
        let Some(obj) = value.as_object() else {
            return fail(line, "a line must be a JSON object");
        };
        if obj.contains_key("note") {
            continue;
        }
        if let Some(t) = obj.get("t") {
            let Some(t) = t.as_u64() else {
                return fail(line, "t must be a non-negative integer");
            };
            if t < now {
                return fail(line, format!("time goes backwards ({t} < {now})"));
            }
            now = t;
        }
        report.steps += 1;

        if let Some(bytes) = obj.get("in") {
            let bytes =
                parse_bytes(bytes).map_err(|m| ReplayError::Fixture { line, message: m })?;
            pipeline.on_midi(now, &bytes, &mut events, &mut out);
            report.messages_in += 1;
        } else if obj.contains_key("tick") {
            pipeline.tick(now, &mut events, &mut out);
        } else if let Some(fb) = obj.get("feedback") {
            let target: FeedbackTarget = match fb.get("target").cloned().map(serde_json::from_value)
            {
                Some(Ok(t)) => t,
                Some(Err(e)) => return fail(line, format!("bad feedback target: {e}")),
                None => return fail(line, "feedback needs a target"),
            };
            let state: FeedbackValue = match fb.get("value").cloned().map(serde_json::from_value) {
                Some(Ok(v)) => v,
                Some(Err(e)) => return fail(line, format!("bad feedback value: {e}")),
                None => return fail(line, "feedback needs a value"),
            };
            pipeline.set_feedback(target, state, now, &mut out);
        } else if let Some(expected) = obj.get("expect") {
            let Some(expected) = expected.as_array() else {
                return fail(line, "expect must be an array");
            };
            let actual: Vec<Value> = events
                .drain(..)
                .map(|e| serde_json::to_value(e).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()
                .map_err(|m| ReplayError::Fixture { line, message: m })?;
            if actual.len() != expected.len() {
                return fail(
                    line,
                    format!(
                        "expected {} event(s), got {}: {}",
                        expected.len(),
                        actual.len(),
                        Value::Array(actual)
                    ),
                );
            }
            for (i, (want, got)) in expected.iter().zip(&actual).enumerate() {
                if let Err(m) = subset_close(want, got) {
                    return fail(line, format!("event {i}: {m}; got {got}"));
                }
            }
            report.checks += 1;
        } else if let Some(expected) = obj.get("expect_out") {
            let Some(expected) = expected.as_array() else {
                return fail(line, "expect_out must be an array of byte arrays");
            };
            let want: Vec<Vec<u8>> = expected
                .iter()
                .map(parse_bytes)
                .collect::<Result<_, _>>()
                .map_err(|m| ReplayError::Fixture { line, message: m })?;
            let got: Vec<Vec<u8>> = out.drain(..).map(|m| m.bytes).collect();
            if want != got {
                return fail(
                    line,
                    format!("expected out {}, got {}", hex_list(&want), hex_list(&got)),
                );
            }
            report.checks += 1;
        } else if let Some(expected) = obj.get("expect_modifiers") {
            let Some(expected) = expected.as_object() else {
                return fail(line, "expect_modifiers must be an object");
            };
            for (name, want) in expected {
                let Some(want) = want.as_bool() else {
                    return fail(line, "modifier states are booleans");
                };
                if pipeline.input().modifier_active(name) != want {
                    return fail(line, format!("modifier {name} should be {want}"));
                }
            }
            report.checks += 1;
        } else {
            return fail(line, "unknown line kind");
        }
    }

    if !events.is_empty() {
        return fail(
            last_line,
            format!("{} unexpected event(s) left at the end", events.len()),
        );
    }
    if !out.is_empty() {
        let left: Vec<Vec<u8>> = out.into_iter().map(|m| m.bytes).collect();
        return fail(
            last_line,
            format!("unexpected output left at the end: {}", hex_list(&left)),
        );
    }
    Ok(report)
}

fn parse_bytes(v: &Value) -> Result<Vec<u8>, String> {
    let Some(arr) = v.as_array() else {
        return Err("bytes must be an array of integers".into());
    };
    arr.iter()
        .map(|b| {
            b.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| "bytes must be integers 0-255".to_owned())
        })
        .collect()
}

fn hex_list(msgs: &[Vec<u8>]) -> String {
    let parts: Vec<String> = msgs
        .iter()
        .map(|m| {
            m.iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// `want` must match `got`: objects match on the keys `want` has, numbers
/// within 1e-4, everything else exactly.
fn subset_close(want: &Value, got: &Value) -> Result<(), String> {
    match (want, got) {
        (Value::Object(w), Value::Object(g)) => {
            for (k, wv) in w {
                match g.get(k) {
                    Some(gv) => subset_close(wv, gv).map_err(|m| format!("{k}: {m}"))?,
                    None => return Err(format!("missing key {k}")),
                }
            }
            Ok(())
        }
        (Value::Number(w), Value::Number(g)) => {
            let (w, g) = (
                w.as_f64().unwrap_or(f64::NAN),
                g.as_f64().unwrap_or(f64::NAN),
            );
            if (w - g).abs() <= 1e-4 {
                Ok(())
            } else {
                Err(format!("expected {w}, got {g}"))
            }
        }
        (w, g) if w == g => Ok(()),
        (w, g) => Err(format!("expected {w}, got {g}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r#"{
        "format_version": 1, "id": "test", "version": 1, "name": "T", "source": "t",
        "devices": { "input": ["T"] },
        "controls": [
          { "id": "play", "midi": { "type": "note", "channel": 1, "number": 11 },
            "action": { "action": "play_pause", "deck": 0 }, "value": { "kind": "button" } }
        ],
        "feedback": [
          { "target": { "target": "playing", "deck": 0 }, "midi": { "type": "note", "channel": 1, "number": 11 },
            "output": { "kind": "light", "on": 127, "off": 0 } }
        ]
    }"#;

    fn mapping() -> Mapping {
        Mapping::from_json(MAP).expect("map")
    }

    #[test]
    fn passing_fixture() {
        let fixture = r#"
{"note": "press and release"}
{"t": 0, "in": [144, 11, 127]}
{"t": 10, "in": [144, 11, 0]}
{"expect": [{"action": {"action": "play_pause", "deck": 0}, "value": {"kind": "button", "value": true}, "timestamp_us": 0}, {"action": {"action": "play_pause", "deck": 0}, "value": {"kind": "button", "value": false}}]}
{"t": 20, "feedback": {"target": {"target": "playing", "deck": 0}, "value": {"kind": "on"}}}
{"expect_out": [[144, 11, 127]]}
"#;
        let report = run(&mapping(), fixture).expect("passes");
        assert_eq!(report.messages_in, 2);
        assert_eq!(report.checks, 2);
    }

    #[test]
    fn wrong_expectation_fails_with_line() {
        let fixture = "{\"t\":0,\"in\":[144,11,127]}\n{\"expect\":[]}\n";
        let err = run(&mapping(), fixture).expect_err("fails");
        assert!(err.to_string().contains("line 2"), "{err}");
    }

    #[test]
    fn leftover_events_fail() {
        let err = run(&mapping(), "{\"t\":0,\"in\":[144,11,127]}\n").expect_err("fails");
        assert!(err.to_string().contains("unexpected event"), "{err}");
        let err = run(
            &mapping(),
            "{\"t\":0,\"feedback\":{\"target\":{\"target\":\"playing\",\"deck\":0},\"value\":{\"kind\":\"on\"}}}\n",
        )
        .expect_err("fails");
        assert!(err.to_string().contains("unexpected output"), "{err}");
    }

    #[test]
    fn time_must_not_go_backwards_and_junk_is_rejected() {
        assert!(
            run(
                &mapping(),
                "{\"t\":5,\"tick\":true}\n{\"t\":4,\"tick\":true}\n"
            )
            .is_err()
        );
        assert!(run(&mapping(), "not json\n").is_err());
        assert!(run(&mapping(), "{\"what\":1}\n").is_err());
    }

    #[test]
    fn number_tolerance() {
        assert!(subset_close(&serde_json::json!(0.5), &serde_json::json!(0.50001)).is_ok());
        assert!(subset_close(&serde_json::json!(0.5), &serde_json::json!(0.6)).is_err());
    }
}
