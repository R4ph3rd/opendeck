//! FX audible effect through the offline renderer: a chain on deck A changes
//! deck A and nothing else.

use std::path::Path;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, CoreEvent, FxAssign, FxOp, LaunchOptions, SettingsStore,
};
use opendeck_contracts::DeckId;
use opendeck_contracts::mixer::CrossfaderAssign;

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn write_sine(path: &Path, freq: f64, secs: f64) {
    let frames = (f64::from(RATE) * secs) as usize;
    let data_len = (frames * 4) as u32;
    let mut bytes = Vec::with_capacity(44 + frames * 4);
    bytes.extend(b"RIFF");
    bytes.extend((36 + data_len).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(RATE.to_le_bytes());
    bytes.extend((RATE * 4).to_le_bytes());
    bytes.extend(4u16.to_le_bytes());
    bytes.extend(16u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_len.to_le_bytes());
    for i in 0..frames {
        let s = 0.5 * (std::f64::consts::TAU * freq * i as f64 / f64::from(RATE)).sin();
        let q = (s * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn launch() -> (AudioController, Receiver<CoreEvent>) {
    let core = AppCore::launch(LaunchOptions {
        backend: Backend::Offline { sample_rate: RATE },
        store: SettingsStore::memory(),
        ..LaunchOptions::default()
    })
    .expect("launch");
    let audio = core.audio().expect("audio");
    let events = audio.subscribe().expect("subscribe");
    (audio, events)
}

fn deck(i: u8) -> DeckId {
    DeckId::new(i).expect("deck")
}

fn load(audio: &AudioController, events: &Receiver<CoreEvent>, d: DeckId, path: &Path) {
    audio.load_track_with_grid(d, path, None).expect("load");
    let deadline = Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(CoreEvent::TrackLoaded(t)) if t.deck == d => return,
            Ok(CoreEvent::TrackLoadFailed(f)) => panic!("load failed: {}", f.message),
            Ok(_) => {}
            Err(_) => panic!("timed out waiting for the load"),
        }
    }
}

fn rms(samples: &[f32]) -> f64 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s).powi(2)).sum();
    (sum / samples.len().max(1) as f64).sqrt()
}

/// Renders `secs` after a short settle so queued commands and installs land.
fn level(audio: &AudioController) -> f64 {
    // The control thread installs on its 16 ms tick; give it wall time too.
    std::thread::sleep(Duration::from_millis(80));
    audio.render_offline(RATE as usize / 5).expect("render");
    rms(&audio
        .render_offline(RATE as usize / 2)
        .expect("render")
        .master)
}

#[test]
fn sound_color_fx_on_deck_a_leaves_deck_b_alone() {
    let dir = std::env::temp_dir().join(format!("opendeck-fxaudio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let (a, b) = (dir.join("a.wav"), dir.join("b.wav"));
    write_sine(&a, 5000.0, 10.0);
    write_sine(&b, 5000.0, 10.0);
    let (audio, events) = launch();
    load(&audio, &events, deck(0), &a);
    load(&audio, &events, deck(1), &b);
    audio
        .set_crossfader_assign(deck(0), CrossfaderAssign::Thru)
        .expect("thru");
    audio
        .set_crossfader_assign(deck(1), CrossfaderAssign::Thru)
        .expect("thru");
    audio.set_playing(deck(0), true).expect("play");
    audio.set_playing(deck(1), true).expect("play");
    let both = level(&audio);
    assert!(both > 0.2, "{both}");

    // Low-pass sweep on channel 2: deck B nearly disappears, deck A stays.
    audio
        .fx_apply(FxOp::SetCfx {
            deck: deck(1),
            amount: -1.0,
        })
        .unwrap();
    let b_filtered = level(&audio);
    assert!(
        b_filtered < both * 0.75 && b_filtered > both * 0.35,
        "deck B filtered: {b_filtered} vs {both}"
    );
    // Back to neutral: centre leaves the sound unchanged.
    audio
        .fx_apply(FxOp::SetCfx {
            deck: deck(1),
            amount: 0.0,
        })
        .unwrap();
    let neutral = level(&audio);
    assert!(
        (neutral - both).abs() < both * 0.05,
        "centre is neutral: {neutral} vs {both}"
    );
    // Both channels filtered: only a trace of the 5 kHz sines remains.
    audio
        .fx_apply(FxOp::SetCfx {
            deck: deck(0),
            amount: -1.0,
        })
        .unwrap();
    audio
        .fx_apply(FxOp::SetCfx {
            deck: deck(1),
            amount: -1.0,
        })
        .unwrap();
    let all = level(&audio);
    assert!(all < b_filtered * 0.5, "both filtered: {all}");
    // Beat FX assignment is untouched by CFX use.
    assert_eq!(audio.fx_state().unwrap().units[0].assign, FxAssign::Ch1);
    let _ = std::fs::remove_dir_all(&dir);
}
