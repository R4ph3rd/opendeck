//! Replay harness over the DDJ-FLX4 mapping (ADR 0007): every fixture in
//! `tests/fixtures/flx4/*.jsonl` is raw MIDI in, expected `ControlEvent`s and
//! LED messages out.

use std::path::PathBuf;

use opendeck_midi::replay;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flx4")
}

fn fixtures() -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = std::fs::read_dir(fixture_dir())
        .expect("fixture directory exists")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "jsonl").then_some(path)
        })
        .map(|path| {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_owned();
            let text = std::fs::read_to_string(&path).expect("fixture is readable");
            (name, text)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_flx4_fixture_replays() {
    let mapping = opendeck_midi::flx4_mapping().expect("FLX4 mapping loads");
    let fixtures = fixtures();
    assert!(fixtures.len() >= 18, "found {} fixtures", fixtures.len());
    let mut checks = 0;
    let mut messages = 0;
    for (name, text) in &fixtures {
        let report = replay::run(&mapping, text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(report.checks > 0, "{name} checks nothing");
        checks += report.checks;
        messages += report.messages_in;
    }
    println!(
        "{} fixtures, {messages} messages in, {checks} expectations",
        fixtures.len()
    );
}

/// One test per fixture, so a failure names the area of the message list.
macro_rules! fixture_test {
    ($test:ident, $file:literal) => {
        #[test]
        fn $test() {
            let mapping = opendeck_midi::flx4_mapping().expect("FLX4 mapping loads");
            let text = std::fs::read_to_string(fixture_dir().join(concat!($file, ".jsonl")))
                .expect("fixture exists");
            replay::run(&mapping, &text).unwrap_or_else(|e| panic!("{}: {e}", $file));
        }
    };
}

fixture_test!(play_cue_shift, "01_play_cue_shift");
fixture_test!(shift_modifiers, "02_shift_modifiers");
fixture_test!(loops_call_sync, "03_loops_call_sync");
fixture_test!(jog, "04_jog");
fixture_test!(tempo_14bit, "05_tempo_14bit");
fixture_test!(eq_trim_cfx, "06_eq_trim_cfx");
fixture_test!(master_headphones, "07_master_headphones");
fixture_test!(channel_fader_crossfader, "08_channel_fader_crossfader");
fixture_test!(channel_cue, "09_channel_cue");
fixture_test!(browse_load, "10_browse_load");
fixture_test!(beat_fx, "11_beat_fx");
fixture_test!(hot_cue_pads, "12_hot_cue_pads");
fixture_test!(pads_all_modes, "13_pads_all_modes");
fixture_test!(pad_mode_buttons, "14_pad_mode_buttons");
fixture_test!(unmapped_and_noise, "15_unmapped_and_noise");
fixture_test!(led_transport, "20_led_transport");
fixture_test!(led_pads, "21_led_pads");
fixture_test!(led_misc, "22_led_misc");

/// The harness must fail when the mapping is wrong, or it proves nothing.
#[test]
fn a_broken_mapping_fails_the_fixtures() {
    let mut mapping = opendeck_midi::flx4_mapping().expect("FLX4 mapping loads");
    let play = mapping
        .controls
        .iter_mut()
        .find(|c| c.id == "deck1.play")
        .expect("play control");
    play.midi = opendeck_midi::mapping::MidiInput::Note {
        channel: 1,
        number: 0x0A,
    };
    let text = std::fs::read_to_string(fixture_dir().join("01_play_cue_shift.jsonl"))
        .expect("fixture exists");
    assert!(replay::run(&mapping, &text).is_err());
}
