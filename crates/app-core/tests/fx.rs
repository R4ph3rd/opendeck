//! FX control model through `AudioController` on the offline backend.
//!
//! Telemetry reports per FX unit whether a slot is enabled or processing, so
//! the tests read which deck chains react.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreEvent, FxAssign, FxBpmSource, FxOp, FxSlotRef, FxTarget,
    FxUnitRef, LaunchOptions, SettingsStore,
};
use opendeck_contracts::fx::{EffectKey, ParamId};
use opendeck_contracts::{DeckId, EngineTelemetry};
use opendeck_library::Library;

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-fx-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn launch_with(library: Option<Library>) -> (AudioController, Receiver<CoreEvent>) {
    let core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        store: SettingsStore::memory(),
        library,
        ..LaunchOptions::default()
    })
    .expect("launch");
    let audio = core.audio().expect("audio");
    let events = audio.subscribe().expect("subscribe");
    (audio, events)
}

fn launch() -> (AudioController, Receiver<CoreEvent>) {
    launch_with(None)
}

fn deck(index: u8) -> DeckId {
    DeckId::new(index).expect("deck")
}

fn builtin(id: &str) -> EffectKey {
    EffectKey {
        provider: "builtin".into(),
        id: id.into(),
    }
}

fn beat(unit: u8) -> FxUnitRef {
    FxUnitRef {
        target: FxTarget::Beat,
        unit,
    }
}

fn slot(unit: u8, slot: u8) -> FxSlotRef {
    FxSlotRef {
        unit: beat(unit),
        slot,
    }
}

fn wait_telemetry(
    events: &Receiver<CoreEvent>,
    ok: impl Fn(&EngineTelemetry) -> bool,
) -> EngineTelemetry {
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(CoreEvent::Telemetry(t)) if ok(&t) => return *t,
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for telemetry"),
        }
    }
}

/// Telemetry arrives about every 16 ms; the engine publishes only changes.
fn settle(events: &Receiver<CoreEvent>) {
    let _ = wait_telemetry(events, |_| true);
}

#[test]
fn unit_on_changes_its_own_deck_chain_only() {
    let (audio, events) = launch();
    settle(&events);
    // Default: unit 0 is on channel 1, unit 1 on channel 2.
    audio
        .fx_apply(FxOp::SetUnitOn { unit: 0, on: true })
        .unwrap();
    let t = wait_telemetry(&events, |t| t.fx_decks[0].enabled);
    assert!(!t.fx_decks[1].enabled, "deck B chain stayed off");
    audio
        .fx_apply(FxOp::SetUnitOn { unit: 0, on: false })
        .unwrap();
    audio
        .fx_apply(FxOp::SetUnitOn { unit: 1, on: true })
        .unwrap();
    let t = wait_telemetry(&events, |t| t.fx_decks[1].enabled && !t.fx_decks[0].enabled);
    assert!(!t.fx_decks[0].enabled);
}

#[test]
fn assigning_both_mirrors_to_both_decks_and_takes_the_channel() {
    let (audio, events) = launch();
    settle(&events);
    audio
        .fx_apply(FxOp::SetAssign {
            unit: 0,
            assign: FxAssign::Both,
        })
        .unwrap();
    let state = audio.fx_state().unwrap();
    assert_eq!(state.units[0].assign, FxAssign::Both);
    assert_eq!(state.units[1].assign, FxAssign::None);
    audio
        .fx_apply(FxOp::SetUnitOn { unit: 0, on: true })
        .unwrap();
    wait_telemetry(&events, |t| t.fx_decks[0].enabled && t.fx_decks[1].enabled);
    // Unit 1 gets channel 2 back and the mirror breaks.
    audio
        .fx_apply(FxOp::SetAssign {
            unit: 1,
            assign: FxAssign::Ch2,
        })
        .unwrap();
    let state = audio.fx_state().unwrap();
    assert_eq!(state.units[0].assign, FxAssign::Ch1);
    assert_eq!(state.units[1].assign, FxAssign::Ch2);
}

#[test]
fn effect_swap_and_level_reach_the_state() {
    let (audio, events) = launch();
    settle(&events);
    audio
        .fx_apply(FxOp::SetEffect {
            slot: slot(0, 0),
            effect: Some(builtin("reverb")),
        })
        .unwrap();
    audio
        .fx_apply(FxOp::SetLevel {
            slot: slot(0, 0),
            value: 0.8,
        })
        .unwrap();
    let state = audio.fx_state().unwrap();
    let s = &state.units[0].slots[0];
    assert_eq!(s.effect, Some(builtin("reverb")));
    let effects = audio.fx_effects().unwrap();
    let reverb = effects.iter().find(|e| e.key == builtin("reverb")).unwrap();
    let primary = reverb.primary.expect("primary parameter");
    assert_eq!(s.params[primary.0 as usize], 0.8);
    // Unknown effects and bad slots are refused.
    assert!(
        audio
            .fx_apply(FxOp::SetEffect {
                slot: slot(0, 0),
                effect: Some(builtin("nope")),
            })
            .is_err()
    );
    assert!(
        audio
            .fx_apply(FxOp::SetParam {
                slot: slot(0, 7),
                param: ParamId(0),
                value: 0.5,
            })
            .is_err()
    );
    // The new effect is installed and runs on switch-on.
    audio
        .fx_apply(FxOp::SetUnitOn { unit: 0, on: true })
        .unwrap();
    wait_telemetry(&events, |t| t.fx_decks[0].enabled);
}

#[test]
fn division_steps_through_the_effect_list() {
    let (audio, _events) = launch();
    let effects = audio.fx_effects().unwrap();
    let echo = effects.iter().find(|e| e.key == builtin("echo")).unwrap();
    let before = audio.fx_state().unwrap().units[0].slots[0].division;
    let at = echo
        .divisions
        .iter()
        .position(|d| (*d - before).abs() < 1e-9)
        .expect("default division is in the list");
    audio
        .fx_apply(FxOp::StepDivision {
            unit: beat(0),
            direction: 1,
        })
        .unwrap();
    let after = audio.fx_state().unwrap().units[0].slots[0].division;
    assert_eq!(
        after,
        echo.divisions[(at + 1).min(echo.divisions.len() - 1)]
    );
    for _ in 0..40 {
        audio
            .fx_apply(FxOp::StepDivision {
                unit: beat(0),
                direction: -1,
            })
            .unwrap();
    }
    let lowest = audio.fx_state().unwrap().units[0].slots[0].division;
    assert_eq!(lowest, echo.divisions[0]);
}

#[test]
fn release_fx_engages_then_lets_go() {
    let (audio, events) = launch();
    settle(&events);
    audio
        .fx_apply(FxOp::SetReleaseEffect {
            unit: 0,
            effect: Some(builtin("vinyl_brake")),
            hold_ms: 300,
        })
        .unwrap();
    audio
        .fx_apply(FxOp::SetUnitOn { unit: 0, on: true })
        .unwrap();
    wait_telemetry(&events, |t| t.fx_decks[0].enabled);
    audio.fx_apply(FxOp::Release { unit: 0 }).unwrap();
    assert!(
        !audio.fx_state().unwrap().units[0].on,
        "release turns the unit off"
    );
    // The release effect runs for the hold time, then everything is off.
    wait_telemetry(&events, |t| t.fx_decks[0].active);
    wait_telemetry(&events, |t| !t.fx_decks[0].enabled);
    assert!(!audio.fx_state().unwrap().units[1].on);
}

#[test]
fn pad_fx_engages_on_press_and_stops_on_release() {
    let (audio, events) = launch();
    settle(&events);
    audio.fx_pad(deck(0), 0, false, 0, true).unwrap();
    let t = wait_telemetry(&events, |t| t.fx_decks[0].enabled);
    assert!(!t.fx_decks[1].enabled);
    audio.fx_pad(deck(0), 0, false, 0, false).unwrap();
    wait_telemetry(&events, |t| !t.fx_decks[0].enabled);
    assert!(audio.fx_pad(deck(0), 2, false, 0, true).is_err());
}

#[test]
fn tap_tempo_sets_the_unit_to_tap() {
    let (audio, _events) = launch();
    for _ in 0..4 {
        audio.fx_apply(FxOp::Tap { unit: 1 }).unwrap();
        std::thread::sleep(Duration::from_millis(500));
    }
    let u = &audio.fx_state().unwrap().units[1];
    assert_eq!(u.bpm_source, FxBpmSource::Tap);
    let bpm = u.tap_bpm.expect("tapped tempo");
    assert!((bpm - 120.0).abs() < 8.0, "tapped {bpm}");
    audio
        .fx_apply(FxOp::SetBpmSource {
            unit: 1,
            source: FxBpmSource::Auto,
        })
        .unwrap();
    assert_eq!(
        audio.fx_state().unwrap().units[1].bpm_source,
        FxBpmSource::Auto
    );
}

#[test]
fn presets_chains_and_state_survive_a_restart() {
    let dir = tmp_dir("persist");
    let path = dir.join("lib.sqlite3");
    let lib = Library::open(&path).expect("library");
    let (audio, _events) = launch_with(Some(lib.clone()));
    audio
        .fx_apply(FxOp::SetEffect {
            slot: slot(0, 0),
            effect: Some(builtin("delay")),
        })
        .unwrap();
    audio
        .fx_apply(FxOp::SetLevel {
            slot: slot(0, 0),
            value: 0.9,
        })
        .unwrap();
    let presets = audio.fx_save_preset(slot(0, 0), "Wide".to_owned()).unwrap();
    assert!(presets.user.iter().any(|p| p.name == "Wide"));
    assert!(!presets.factory.is_empty());
    let presets = audio.fx_save_chain(beat(0), "Mine".to_owned()).unwrap();
    assert_eq!(presets.chains.len(), 1);
    audio
        .fx_apply(FxOp::SetCfx {
            deck: deck(1),
            amount: -0.5,
        })
        .unwrap();
    let before = audio.fx_state().unwrap();
    drop(audio);

    let (audio, _events) = launch_with(Some(Library::open(&path).expect("reopen")));
    assert_eq!(audio.fx_state().unwrap(), before);
    let presets = audio.fx_presets().unwrap();
    let wide = presets
        .user
        .iter()
        .find(|p| p.name == "Wide")
        .expect("user preset restored");
    assert_eq!(wide.effect, builtin("delay"));
    assert_eq!(presets.chains[0].name, "Mine");
    // Recall the preset and the chain.
    audio
        .fx_apply(FxOp::SetEffect {
            slot: slot(0, 0),
            effect: Some(builtin("echo")),
        })
        .unwrap();
    audio
        .fx_apply(FxOp::ApplyPreset {
            slot: slot(0, 0),
            preset: wide.clone(),
        })
        .unwrap();
    assert_eq!(
        audio.fx_state().unwrap().units[0].slots[0].effect,
        Some(builtin("delay"))
    );
    audio
        .fx_apply(FxOp::ApplyChain {
            unit: beat(1),
            name: "Mine".to_owned(),
        })
        .unwrap();
    assert_eq!(
        audio.fx_state().unwrap().units[1].slots[0].effect,
        Some(builtin("delay"))
    );
    let presets = audio
        .fx_delete_preset(builtin("delay"), "Wide".to_owned())
        .unwrap();
    assert!(presets.user.is_empty());
    assert!(
        audio
            .fx_delete_chain("Mine".to_owned())
            .unwrap()
            .chains
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(dir);
}
