//! Keeps `mappings/schema/mapping.schema.json` in step with the contracts
//! `ControlAction` and `FeedbackTarget` serde forms (ADR 0007).

use opendeck_contracts::control::{ControlAction, FeedbackTarget};
use serde_json::Value;

fn schema() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../mappings/schema/mapping.schema.json"
    );
    let text = std::fs::read_to_string(path).expect("schema file exists");
    serde_json::from_str(&text).expect("schema is valid JSON")
}

fn names(schema: &Value, pointer: &str) -> Vec<String> {
    schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .expect("enum array")
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

/// A tag the enum knows fails with "missing field" (or succeeds), never "unknown variant".
fn is_known_variant<T: serde::de::DeserializeOwned>(tag: &str, name: &str) -> bool {
    let probe = serde_json::json!({ tag: name });
    match serde_json::from_value::<T>(probe) {
        Ok(_) => true,
        Err(e) => !e.to_string().contains("unknown variant"),
    }
}

#[test]
fn schema_actions_exist_in_contracts() {
    let schema = schema();
    let actions = names(&schema, "/$defs/action/properties/action/enum");
    assert!(!actions.is_empty());
    for name in &actions {
        assert!(
            is_known_variant::<ControlAction>("action", name),
            "schema action {name} is not a ControlAction"
        );
    }
}

#[test]
fn schema_feedback_targets_exist_in_contracts() {
    let schema = schema();
    let targets = names(
        &schema,
        "/$defs/feedback/properties/target/properties/target/enum",
    );
    for name in &targets {
        assert!(
            is_known_variant::<FeedbackTarget>("target", name),
            "schema target {name} is not a FeedbackTarget"
        );
    }
}
