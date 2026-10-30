//! Keeps `mappings/schema/mapping.schema.json` in step with the contracts
//! `ControlAction` and `FeedbackTarget` serde forms (ADR 0007), in both
//! directions:
//!
//! - every schema enum value is a contracts variant;
//! - every contracts variant is in the schema enum (the exhaustive `match` in
//!   the macros below stops compiling when a variant is added without a name,
//!   and the name list is generated from the same macro input);
//! - every action and target the DDJ-FLX4 mapping uses is in the schema, and
//!   every variant the FLX4 mapping does not use is on a reviewed list.

use std::collections::BTreeSet;

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

macro_rules! action_names {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Serde names of every `ControlAction` variant.
        const ACTION_NAMES: &[&str] = &[$($name),*];

        /// Exhaustive: a new variant fails to compile here until it is listed.
        fn action_name(action: &ControlAction) -> &'static str {
            match action {
                $(ControlAction::$variant { .. } => $name),*
            }
        }
    };
}

action_names! {
    PlayPause => "play_pause", Cue => "cue", ReturnToStart => "return_to_start", Sync => "sync",
    SyncLeader => "sync_leader", Load => "load", TempoFader => "tempo_fader",
    TempoRangeNext => "tempo_range_next", KeyLock => "key_lock", PitchBend => "pitch_bend",
    JogTouch => "jog_touch", JogTop => "jog_top", JogSide => "jog_side", JogSearch => "jog_search",
    VinylMode => "vinyl_mode", LoopIn => "loop_in", LoopOut => "loop_out", LoopToggle => "loop_toggle",
    AutoLoop => "auto_loop", LoopHalve => "loop_halve", LoopDouble => "loop_double",
    BeatJump => "beat_jump", CueLoopCallPrev => "cue_loop_call_prev",
    CueLoopCallNext => "cue_loop_call_next", MemoryCueAdd => "memory_cue_add",
    MemoryCueDelete => "memory_cue_delete", TapTempo => "tap_tempo", Slip => "slip",
    Quantize => "quantize", PadModeSelect => "pad_mode_select", Pad => "pad", HotCue => "hot_cue",
    HotCueDelete => "hot_cue_delete", Trim => "trim", Eq => "eq", Filter => "filter",
    ChannelFader => "channel_fader", ChannelFaderStart => "channel_fader_start", Pfl => "pfl",
    Crossfader => "crossfader", MasterLevel => "master_level", MasterCue => "master_cue",
    HeadphoneLevel => "headphone_level", HeadphoneMix => "headphone_mix", FxToggle => "fx_toggle",
    FxLevel => "fx_level", FxSelect => "fx_select", FxBeat => "fx_beat", FxRelease => "fx_release",
    FxBpmAuto => "fx_bpm_auto", FxBpmTap => "fx_bpm_tap", FxAssign => "fx_assign",
    Sampler => "sampler", BrowseScroll => "browse_scroll", BrowsePress => "browse_press",
    BrowseBack => "browse_back",
}

macro_rules! target_names {
    ($($variant:ident => $name:literal),* $(,)?) => {
        /// Serde names of every `FeedbackTarget` variant.
        const TARGET_NAMES: &[&str] = &[$($name),*];

        /// Exhaustive, see `action_name`.
        #[allow(dead_code)]
        fn target_name(target: &FeedbackTarget) -> &'static str {
            match target {
                $(FeedbackTarget::$variant { .. } => $name),*
            }
        }
    };
}

target_names! {
    Playing => "playing", CueIndicator => "cue_indicator", SyncOn => "sync_on",
    LoopActive => "loop_active", PflOn => "pfl_on", PadModeActive => "pad_mode_active",
    Pad => "pad", FxOn => "fx_on", ChannelLevel => "channel_level", TrackLoaded => "track_loaded",
    VinylMode => "vinyl_mode",
}

/// A tag the enum knows fails with "missing field" (or succeeds), never "unknown variant".
fn is_known_variant<T: serde::de::DeserializeOwned>(tag: &str, name: &str) -> bool {
    let probe = serde_json::json!({ tag: name });
    match serde_json::from_value::<T>(probe) {
        Ok(_) => true,
        Err(e) => !e.to_string().contains("unknown variant"),
    }
}

const ACTION_POINTER: &str = "/$defs/action/properties/action/enum";
const TARGET_POINTER: &str = "/$defs/feedback/properties/target/properties/target/enum";

#[test]
fn schema_actions_exist_in_contracts() {
    let schema = schema();
    let actions = names(&schema, ACTION_POINTER);
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
    let targets = names(&schema, TARGET_POINTER);
    for name in &targets {
        assert!(
            is_known_variant::<FeedbackTarget>("target", name),
            "schema target {name} is not a FeedbackTarget"
        );
    }
}

#[test]
fn contracts_actions_exist_in_schema() {
    let schema: BTreeSet<String> = names(&schema(), ACTION_POINTER).into_iter().collect();
    let contracts: BTreeSet<String> = ACTION_NAMES.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        contracts.difference(&schema).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "ControlAction variants missing from the schema enum"
    );
    assert_eq!(
        schema.difference(&contracts).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "schema actions missing from the test name list"
    );
    for name in ACTION_NAMES {
        assert!(
            is_known_variant::<ControlAction>("action", name),
            "{name} is not a serde name of ControlAction"
        );
    }
}

#[test]
fn contracts_targets_exist_in_schema() {
    let schema: BTreeSet<String> = names(&schema(), TARGET_POINTER).into_iter().collect();
    let contracts: BTreeSet<String> = TARGET_NAMES.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(schema, contracts);
    for name in TARGET_NAMES {
        assert!(is_known_variant::<FeedbackTarget>("target", name));
    }
}

/// Actions the DDJ-FLX4 has no control for. app-core derives them from other
/// controls (pads, loop state) or they exist for keyboard and UI use.
const NOT_ON_FLX4: &[(&str, &str)] = &[
    ("key_lock", "no KEY LOCK button on the FLX4"),
    ("pitch_bend", "no PITCH BEND buttons; the jog wheel bends"),
    (
        "loop_halve",
        "CUE/LOOP CALL halves and doubles the loop while a loop plays (1-8, 1-9); app-core decides from loop state",
    ),
    ("loop_double", "see loop_halve"),
    (
        "beat_jump",
        "beat jump is a pad mode (5-3), carried by the pad action",
    ),
    ("slip", "no SLIP button on the FLX4"),
    ("quantize", "no QUANTIZE button on the FLX4"),
    (
        "hot_cue",
        "hot cues are pad presses (5-5), carried by the pad action",
    ),
    (
        "hot_cue_delete",
        "SHIFT + hot cue pad (5-5), carried by the pad action with shifted",
    ),
    (
        "sampler",
        "sampler slots are pad presses (5-5), carried by the pad action",
    ),
];

#[test]
fn flx4_actions_are_in_the_schema_and_the_rest_is_reviewed() {
    let mapping = opendeck_midi::flx4_mapping().expect("FLX4 mapping loads");
    let schema: BTreeSet<String> = names(&schema(), ACTION_POINTER).into_iter().collect();
    let used: BTreeSet<&str> = mapping
        .controls
        .iter()
        .map(|c| action_name(&c.action))
        .collect();
    for name in &used {
        assert!(
            schema.contains(*name),
            "FLX4 uses {name}, not in the schema"
        );
    }
    let unused: BTreeSet<&str> = ACTION_NAMES
        .iter()
        .copied()
        .filter(|n| !used.contains(n))
        .collect();
    let reviewed: BTreeSet<&str> = NOT_ON_FLX4.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        unused, reviewed,
        "ControlAction variants not used by the FLX4 mapping must be reviewed in NOT_ON_FLX4 \
         (and no reviewed variant may be in use)"
    );
}

#[test]
fn flx4_feedback_targets_are_all_used() {
    let mapping = opendeck_midi::flx4_mapping().expect("FLX4 mapping loads");
    let used: BTreeSet<&str> = mapping
        .feedback
        .iter()
        .map(|f| target_name(&f.target))
        .collect();
    let all: BTreeSet<&str> = TARGET_NAMES.iter().copied().collect();
    assert_eq!(used, all);
}

/// Property names of each object kind in the mapping file must exist in the
/// schema, so a Rust-only field cannot slip into shipped files.
#[test]
fn flx4_file_only_uses_schema_properties() {
    let schema = schema();
    let text = opendeck_midi::FLX4_MAPPING_JSON;
    let file: Value = serde_json::from_str(text).expect("mapping is JSON");

    let props = |pointer: &str| -> BTreeSet<String> {
        schema
            .pointer(pointer)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("schema has {pointer}"))
            .keys()
            .cloned()
            .collect()
    };
    let check = |what: &str, value: &Value, allowed: &BTreeSet<String>| {
        for key in value.as_object().expect("object").keys() {
            assert!(
                allowed.contains(key),
                "{what}: property {key} is not in the schema"
            );
        }
    };

    check("root", &file, &props("/properties"));
    let modifier = props("/properties/modifiers/items/properties");
    for m in file["modifiers"].as_array().expect("modifiers") {
        check("modifier", m, &modifier);
    }
    let control = props("/$defs/control/properties");
    for c in file["controls"].as_array().expect("controls") {
        check("control", c, &control);
    }
    let feedback = props("/$defs/feedback/properties");
    for f in file["feedback"].as_array().expect("feedback") {
        check("feedback", f, &feedback);
    }
    let unmapped = props("/$defs/unmapped/properties");
    for u in file["unmapped"].as_array().expect("unmapped") {
        check("unmapped", u, &unmapped);
    }
}
