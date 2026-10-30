//! The worker thread against the mock backend: hot-plug, reconnect, feedback
//! replay, monitor and learn. No hardware needed.

use std::time::{Duration, Instant};

use opendeck_contracts::control::{ControlAction, ControlValue, FeedbackTarget, FeedbackValue};
use opendeck_contracts::ids::DeckId;
use opendeck_midi::backend::mock::MockHandle;
use opendeck_midi::learn::LearnHint;
use opendeck_midi::mapping::{Mapping, MidiInput};
use opendeck_midi::monitor::{Direction, MonitorLabel};
use opendeck_midi::{Controller, ControllerConfig};

const PORT: &str = "DDJ-FLX4 MIDI 1";
const WAIT: Duration = Duration::from_secs(3);

/// Two controls, one LED, and a SysEx `on_connect`.
const MAP: &str = r#"{
    "format_version": 1, "id": "test-flx", "version": 1, "name": "T", "source": "test",
    "devices": { "input": ["DDJ-FLX4"], "output": ["ddj-flx4"] },
    "on_connect": [{ "bytes": [240, 126, 127, 6, 1, 247], "comment": "identity request" }],
    "controls": [
      { "id": "deck1.play", "midi": { "type": "note", "channel": 1, "number": 11 },
        "action": { "action": "play_pause", "deck": 0 }, "value": { "kind": "button", "edge": "press" } },
      { "id": "deck1.tempo", "midi": { "type": "cc14", "channel": 1, "msb": 0, "lsb": 32 },
        "action": { "action": "tempo_fader", "deck": 0 }, "value": { "kind": "absolute" } }
    ],
    "feedback": [
      { "target": { "target": "playing", "deck": 0 }, "midi": { "type": "note", "channel": 1, "number": 11 },
        "output": { "kind": "light", "on": 127, "off": 0 } }
    ]
}"#;

fn deck0() -> DeckId {
    DeckId::new(0).expect("deck")
}

fn start(mock: &MockHandle) -> Controller {
    let mapping = Mapping::from_json(MAP).expect("mapping");
    let mut config = ControllerConfig::new(mapping);
    config.rescan_interval = Duration::from_millis(10);
    let backend = mock.clone();
    Controller::start_with(
        config,
        Box::new(move || Ok(Box::new(backend.backend()) as _)),
    )
    .expect("controller starts")
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn hot_plug_connects_decodes_and_reconnects() {
    let mock = MockHandle::default();
    let controller = start(&mock);
    assert_eq!(controller.status().input, None);

    mock.plug("Other Device");
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        controller.status().input,
        None,
        "a non-matching port is ignored"
    );

    mock.plug(PORT);
    wait_until("input connect", || {
        controller.status().input.as_deref() == Some(PORT)
    });
    assert_eq!(controller.status().output.as_deref(), Some(PORT));

    assert!(mock.push(PORT, 1_000, &[0x90, 11, 127]));
    let event = controller.events().recv_timeout(WAIT).expect("event");
    assert_eq!(event.action, ControlAction::PlayPause { deck: deck0() });
    assert_eq!(event.value, ControlValue::Button(true));

    // 14-bit pair with the driver timestamps 100 us apart.
    mock.push(PORT, 2_000, &[0xB0, 0x00, 0x7F]);
    mock.push(PORT, 2_100, &[0xB0, 0x20, 0x7F]);
    let event = controller.events().recv_timeout(WAIT).expect("tempo");
    assert_eq!(event.value, ControlValue::Absolute(1.0));

    mock.unplug(PORT);
    wait_until("disconnect", || controller.status().input.is_none());
    assert_eq!(controller.status().output, None);
    assert!(!mock.input_open(PORT));

    mock.plug(PORT);
    wait_until("reconnect", || {
        controller.status().input.as_deref() == Some(PORT)
    });
    assert!(mock.push(PORT, 9_000_000, &[0x90, 11, 127]));
    let event = controller
        .events()
        .recv_timeout(WAIT)
        .expect("event after reconnect");
    assert_eq!(event.action, ControlAction::PlayPause { deck: deck0() });
    controller.shutdown();
    assert!(!mock.input_open(PORT), "shutdown closes the input");
}

#[test]
fn msb_alone_is_flushed_by_the_worker_timer() {
    let mock = MockHandle::default();
    mock.plug(PORT);
    let controller = start(&mock);
    wait_until("connect", || controller.status().input.is_some());
    mock.push(PORT, 1_000, &[0xB0, 0x00, 0x40]);
    let event = controller
        .events()
        .recv_timeout(WAIT)
        .expect("MSB-only value arrives after the timeout");
    assert!(matches!(event.value, ControlValue::Absolute(x) if (x - 0.5).abs() < 0.001));
}

#[test]
fn feedback_is_cached_until_the_output_opens_then_sent_after_on_connect() {
    let mock = MockHandle::default();
    let controller = start(&mock);
    let playing = FeedbackTarget::Playing { deck: deck0() };
    controller
        .set_feedback(playing, FeedbackValue::On)
        .expect("queued");
    std::thread::sleep(Duration::from_millis(30));
    assert!(mock.sent().is_empty());

    mock.plug(PORT);
    wait_until("on_connect and LED replay", || mock.sent().len() >= 2);
    let sent = mock.sent();
    assert_eq!(
        sent[0].1,
        vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7],
        "SysEx first"
    );
    assert_eq!(sent[1].1, vec![0x90, 11, 127], "then the cached LED state");

    // De-duplication and change.
    mock.clear_sent();
    controller
        .set_feedback(playing, FeedbackValue::On)
        .expect("queued");
    controller
        .set_feedback(playing, FeedbackValue::Off)
        .expect("queued");
    wait_until("LED off", || !mock.sent().is_empty());
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(
        mock.sent().into_iter().map(|(_, b)| b).collect::<Vec<_>>(),
        vec![vec![0x90, 11, 0]]
    );

    // A replugged device gets the state again.
    mock.unplug(PORT);
    wait_until("disconnect", || controller.status().output.is_none());
    mock.clear_sent();
    mock.plug(PORT);
    wait_until("replay after replug", || mock.sent().len() >= 2);
    assert_eq!(mock.sent()[1].1, vec![0x90, 11, 0]);
}

#[test]
fn monitor_shows_traffic_in_both_directions_with_labels() {
    let mock = MockHandle::default();
    mock.plug(PORT);
    let controller = start(&mock);
    wait_until("connect", || controller.status().input.is_some());
    controller.set_monitor(true).expect("monitor on");
    std::thread::sleep(Duration::from_millis(30));

    mock.push(PORT, 1_000, &[0x90, 11, 127]);
    mock.push(PORT, 2_000, &[0x90, 99, 127]);
    controller
        .set_feedback(FeedbackTarget::Playing { deck: deck0() }, FeedbackValue::On)
        .expect("queued");

    let mut seen = Vec::new();
    while seen.len() < 3 {
        seen.push(
            controller
                .monitor()
                .recv_timeout(WAIT)
                .expect("monitor event"),
        );
    }
    let input = seen
        .iter()
        .find(|e| e.bytes == [0x90, 11, 127] && e.direction == Direction::In)
        .expect("play press");
    assert_eq!(
        input.label,
        MonitorLabel::Control {
            id: "deck1.play".into(),
            verify: None
        }
    );
    assert_eq!(input.text, "Note On ch1 0x0B vel 127");
    let unmapped = seen
        .iter()
        .find(|e| e.bytes == [0x90, 99, 127])
        .expect("unmapped note");
    assert_eq!(unmapped.label, MonitorLabel::Unmapped);
    assert!(
        seen.iter()
            .any(|e| e.direction == Direction::Out && e.bytes == [0x90, 11, 127])
    );
    assert!(
        seen.windows(2)
            .all(|w| w[0].ts_us <= w[1].ts_us || w[0].direction != w[1].direction)
    );
}

#[test]
fn learn_captures_the_next_gesture_instead_of_mapping_it() {
    let mock = MockHandle::default();
    mock.plug(PORT);
    let controller = start(&mock);
    wait_until("connect", || controller.status().input.is_some());

    controller.start_learn(LearnHint::Auto).expect("armed");
    std::thread::sleep(Duration::from_millis(30));
    mock.push(PORT, 1_000, &[0x92, 0x05, 0x7F]);
    let capture = controller.learned().recv_timeout(WAIT).expect("capture");
    assert_eq!(
        capture.midi,
        MidiInput::Note {
            channel: 3,
            number: 5
        }
    );

    // Disarmed again: mapped controls work, and the gesture produced no event.
    assert!(controller.events().try_recv().is_err());
    mock.push(PORT, 2_000, &[0x90, 11, 127]);
    assert_eq!(
        controller
            .events()
            .recv_timeout(WAIT)
            .expect("event")
            .action,
        ControlAction::PlayPause { deck: deck0() }
    );
}

#[test]
fn cc_learn_finishes_on_the_worker_timer() {
    let mock = MockHandle::default();
    mock.plug(PORT);
    let controller = start(&mock);
    wait_until("connect", || controller.status().input.is_some());
    controller.start_learn(LearnHint::Auto).expect("armed");
    std::thread::sleep(Duration::from_millis(30));
    mock.push(PORT, 1_000, &[0xB6, 0x40, 0x01]);
    mock.push(PORT, 2_000, &[0xB6, 0x40, 0x7F]);
    let capture = controller
        .learned()
        .recv_timeout(WAIT)
        .expect("capture after the window");
    assert_eq!(
        capture.midi,
        MidiInput::Cc {
            channel: 7,
            number: 0x40
        }
    );
}

#[test]
fn open_failures_are_reported_and_retried() {
    let mock = MockHandle::default();
    mock.plug(PORT);
    mock.set_fail_open(true);
    let controller = start(&mock);
    wait_until("error reported", || {
        controller.status().last_error.is_some()
    });
    assert_eq!(controller.status().input, None);
    mock.set_fail_open(false);
    wait_until("connected after retry", || {
        controller.status().input.is_some()
    });
    assert_eq!(controller.status().last_error, None);
}
