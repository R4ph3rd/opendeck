//! App-level MIDI replay: the midi crate's recorded DDJ-FLX4 messages go
//! through the real mapping `Pipeline`, the `MidiBridge` and a headless
//! `AppCore` (offline renderer, in-memory library). The tests assert state
//! changes (deck plays, hot cue set, EQ, load from browse, FX on, pad mode)
//! and the LED messages that leave the pipeline after those changes.
//!
//! Time is explicit: the pipeline clock only moves when a test advances it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use opendeck_app_core::{
    AppCore, AudioController, Backend, BridgeNotice, CoreEvent, LaunchOptions, LibraryService,
    MidiBridge, SettingsStore,
};
use opendeck_contracts::DeckId;
use opendeck_contracts::mixer::EqBand;
use opendeck_contracts::pad::PadMode;
use opendeck_library::import::ImportConfig;
use opendeck_midi::{FeedbackConfig, Pipeline, flx4_mapping};

const RATE: u32 = 48_000;
const WAIT: Duration = Duration::from_secs(10);

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("opendeck-midi-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_wav(path: &Path, secs: f32, hz: f64) {
    let frames = (RATE as f32 * secs) as usize;
    let data_len = (frames * 4) as u32;
    let mut bytes = Vec::new();
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
        let s = (std::f64::consts::TAU * hz * i as f64 / f64::from(RATE)).sin() * 0.5;
        let q = (s * 32767.0) as i16;
        bytes.extend(q.to_le_bytes());
        bytes.extend(q.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write wav");
}

fn deck(n: u8) -> DeckId {
    DeckId::new(n).expect("deck")
}

struct Rig {
    core: AppCore,
    audio: AudioController,
    events: Receiver<CoreEvent>,
    bridge: MidiBridge,
    pipeline: Pipeline,
    now_us: u64,
    telemetry: Option<opendeck_contracts::EngineTelemetry>,
    /// Library track of the newest `TrackLoaded` per deck.
    loaded: [Option<opendeck_contracts::ids::TrackId>; 2],
    /// LED messages since the last `take_out`.
    out: Vec<Vec<u8>>,
    notices: Vec<BridgeNotice>,
}

impl Rig {
    /// Two tracks in the library, both decks loaded, nothing playing.
    fn new(name: &str) -> Self {
        let dir = tmp_dir(name);
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        write_wav(&a, 4.0, 220.0);
        write_wav(&b, 4.0, 330.0);
        let library =
            LibraryService::open_in_memory(Vec::new(), ImportConfig::default()).expect("library");
        let core = AppCore::launch_with_library(
            LaunchOptions {
                backend: Backend::Offline { sample_rate: RATE },
                store: SettingsStore::memory(),
                ..LaunchOptions::default()
            },
            library.clone(),
        )
        .expect("launch");
        let audio = core.audio().expect("audio");
        let events = audio.subscribe().expect("subscribe");
        let bridge = MidiBridge::new(audio.clone(), Some(library), core.pad_modes());
        let pipeline = Pipeline::new(flx4_mapping().expect("mapping"), FeedbackConfig::default())
            .expect("pipeline");
        let mut rig = Self {
            core,
            audio,
            events,
            bridge,
            pipeline,
            now_us: 1_000,
            telemetry: None,
            loaded: [None; 2],
            out: Vec::new(),
            notices: Vec::new(),
        };
        rig.core.load_track_path(deck(0), &a).expect("load a");
        rig.core.load_track_path(deck(1), &b).expect("load b");
        rig.wait("both decks loaded", |r| {
            r.telemetry
                .is_some_and(|t| t.decks[0].loaded && t.decks[1].loaded)
        });
        rig.bridge.refresh();
        rig.settle();
        rig
    }

    /// Takes in engine events and sends the LED changes they cause.
    fn settle(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match &event {
                CoreEvent::Telemetry(t) => self.telemetry = Some(**t),
                CoreEvent::TrackLoaded(l) if l.deck.index() < 2 => {
                    self.loaded[l.deck.index()] = l.track.id;
                }
                _ => {}
            }
            self.bridge.observe(&event);
        }
        let mut out = Vec::new();
        for (target, value) in self.bridge.feedback() {
            self.pipeline
                .set_feedback(target, value, self.now_us, &mut out);
        }
        self.out.extend(out.into_iter().map(|m| m.bytes));
    }

    /// One message from the controller, 2 ms after the previous one.
    fn send(&mut self, bytes: [u8; 3]) {
        self.now_us += 2_000;
        let mut events = Vec::new();
        let mut out = Vec::new();
        self.pipeline
            .on_midi(self.now_us, &bytes, &mut events, &mut out);
        self.out.extend(out.into_iter().map(|m| m.bytes));
        for event in events {
            let notices = self.bridge.handle(&event);
            self.notices.extend(notices);
        }
        self.settle();
    }

    /// Moves the pipeline clock and runs its timers (blink, rate limits).
    fn advance(&mut self, micros: u64) {
        let step = 50_000;
        let mut left = micros;
        while left > 0 {
            let dt = left.min(step);
            self.now_us += dt;
            left -= dt;
            let mut events = Vec::new();
            let mut out = Vec::new();
            self.pipeline.tick(self.now_us, &mut events, &mut out);
            self.out.extend(out.into_iter().map(|m| m.bytes));
            for event in events {
                let notices = self.bridge.handle(&event);
                self.notices.extend(notices);
            }
            self.settle();
        }
    }

    fn wait(&mut self, what: &str, cond: impl Fn(&mut Rig) -> bool) {
        let deadline = Instant::now() + WAIT;
        loop {
            self.now_us += 20_000;
            self.settle();
            if cond(self) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn led_seen(&self, bytes: [u8; 3]) -> bool {
        self.out.iter().any(|m| m.as_slice() == bytes)
    }

    fn take_out(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.out)
    }

    fn playing(&self, deck: usize) -> bool {
        self.telemetry.is_some_and(|t| t.decks[deck].playing)
    }
}

#[test]
fn play_button_starts_the_deck_and_the_led_follows() {
    let mut rig = Rig::new("play");
    // Paused with a track loaded: the PLAY LED blinks (software timer), and
    // the vinyl and mode LEDs were lit when the state arrived.
    assert!(rig.led_seen([144, 11, 127]), "PLAY blink on");
    assert!(rig.led_seen([144, 23, 127]), "vinyl mode lit");
    assert!(rig.led_seen([144, 27, 127]), "HOT CUE mode lit");
    assert!(rig.led_seen([159, 0, 127]), "deck 1 track loaded");

    rig.send([144, 11, 127]);
    rig.send([144, 11, 0]);
    rig.wait("deck 1 plays", |r| r.playing(0));
    assert!(!rig.playing(1));

    // Pause again and let the blink run through an off phase.
    rig.take_out();
    rig.send([144, 11, 127]);
    rig.wait("deck 1 pauses", |r| !r.playing(0));
    rig.advance(1_200_000);
    assert!(rig.led_seen([144, 11, 0]), "blink off phase after a pause");
    assert!(rig.led_seen([144, 14, 0]), "the +SHIFT bank mirrors it");
}

#[test]
fn hot_cue_pad_sets_and_deletes_a_cue_and_lights_the_pad() {
    let mut rig = Rig::new("hotcue");
    assert!(rig.audio.hot_cues(deck(0)).expect("cues").slots[0].is_none());
    rig.take_out();
    rig.send([151, 0, 127]);
    rig.send([151, 0, 0]);
    rig.wait("hot cue 1 set", |r| {
        r.audio.hot_cues(deck(0)).expect("cues").slots[0].is_some()
    });
    rig.wait("pad lit", |r| r.led_seen([151, 0, 127]));
    assert!(
        rig.led_seen([152, 0, 127]),
        "the +SHIFT pad bank is lit too"
    );

    // SHIFT + pad deletes it.
    rig.send([152, 0, 127]);
    rig.send([152, 0, 0]);
    rig.wait("hot cue 1 deleted", |r| {
        r.audio.hot_cues(deck(0)).expect("cues").slots[0].is_none()
    });
    rig.wait("pad dark", |r| r.led_seen([151, 0, 0]));
}

#[test]
fn eq_knob_sets_the_mixer_band() {
    let mut rig = Rig::new("eq");
    // EQ HI of channel 1 is a 14-bit CC pair (MSB 7, LSB 39).
    rig.send([176, 7, 127]);
    rig.send([176, 39, 127]);
    let boost = 10f32.powf(opendeck_contracts::mixer::EQ_MAX_GAIN_DB / 20.0);
    rig.wait("EQ high boosted", |r| {
        let gain = r.audio.mixer_state().expect("mixer").channels[0].eq[EqBand::High.index()];
        (gain.0 - boost).abs() < 0.01
    });
    rig.send([176, 7, 0]);
    rig.send([176, 39, 0]);
    rig.wait("EQ high killed", |r| {
        r.audio.mixer_state().expect("mixer").channels[0].eq[EqBand::High.index()].0 < 0.001
    });
}

#[test]
fn browse_encoder_and_load_put_the_selected_track_on_the_deck() {
    let mut rig = Rig::new("browse");
    rig.core.eject(deck(1)).expect("eject");
    rig.wait("deck 2 empty", |r| {
        r.telemetry.is_some_and(|t| !t.decks[1].loaded)
    });
    let before = rig.loaded[1];

    // Turn the encoder one tick, then LOAD deck 2 (ch 7, note 0x47).
    rig.send([182, 64, 1]);
    let cursor = rig
        .notices
        .iter()
        .find_map(|n| match n {
            BridgeNotice::Browse(c) => Some(c.clone()),
            _ => None,
        })
        .expect("the encoder moved the cursor");
    assert_eq!(cursor.index, 0);
    rig.send([150, 71, 127]);
    rig.send([150, 71, 0]);
    rig.wait("deck 2 loaded", |r| {
        r.telemetry.is_some_and(|t| t.decks[1].loaded)
    });
    assert_eq!(rig.loaded[1], Some(cursor.track));
    assert!(before.is_some(), "deck 2 held a library track before");
    rig.wait("track loaded LED", |r| r.led_seen([159, 0, 127]));
}

#[test]
fn beat_fx_section_follows_ch_select() {
    let mut rig = Rig::new("fx");
    let on = |rig: &Rig, unit: usize| rig.audio.fx_state().expect("fx").units[unit].on;

    // Power-on assumption is CH1: FX ON/OFF (ch5 note 0x47) drives the unit on channel 1.
    rig.take_out();
    rig.send([148, 71, 127]);
    rig.send([148, 71, 0]);
    rig.wait("unit 1 on", |r| on(r, 0));
    assert!(!on(&rig, 1));
    rig.wait("FX LED", |r| r.led_seen([148, 71, 127]));
    rig.send([148, 71, 127]);
    rig.wait("unit 1 off", |r| !on(r, 0));
    rig.wait("FX LED off", |r| r.led_seen([148, 71, 0]));

    // CH SELECT to CH2: flags ch5 0x10 = 0, ch6 0x11 = 7F. The unit on channel 2 follows.
    rig.send([148, 16, 0]);
    rig.send([149, 17, 127]);
    rig.send([149, 71, 127]);
    rig.wait("unit 2 on", |r| on(r, 1));
    assert!(!on(&rig, 0));
    rig.wait("CH2 FX LED", |r| r.led_seen([149, 71, 127]));
    rig.send([149, 71, 127]);
    rig.wait("unit 2 off", |r| !on(r, 1));

    // CH1&CH2: both units.
    rig.send([148, 16, 127]);
    rig.send([148, 71, 127]);
    rig.wait("both units on", |r| on(r, 0) && on(r, 1));
}

#[test]
fn pad_mode_buttons_switch_modes_and_filter_pads() {
    let mut rig = Rig::new("padmode");
    assert_eq!(rig.core.pad_mode(deck(0)), PadMode::HotCue);
    rig.take_out();
    // BEAT LOOP mode button of deck 1 (note 0x6D).
    rig.send([144, 109, 127]);
    rig.send([144, 109, 0]);
    assert_eq!(rig.core.pad_mode(deck(0)), PadMode::BeatLoop);
    assert_eq!(rig.core.pad_mode(deck(1)), PadMode::HotCue, "per deck");
    assert!(
        rig.notices
            .iter()
            .any(|n| matches!(n, BridgeNotice::PadMode(p) if p.mode == PadMode::BeatLoop))
    );
    rig.wait("mode LEDs", |r| {
        r.led_seen([144, 27, 0]) && r.led_seen([144, 109, 127])
    });

    // A hot cue pad press (mode HOT CUE) is ignored in BEAT LOOP mode.
    rig.send([151, 0, 127]);
    rig.send([151, 0, 0]);
    std::thread::sleep(Duration::from_millis(200));
    rig.settle();
    assert!(rig.audio.hot_cues(deck(0)).expect("cues").slots[0].is_none());
}

#[test]
fn headphone_cue_and_vinyl_buttons_toggle_and_light() {
    let mut rig = Rig::new("toggles");
    rig.take_out();
    rig.send([144, 84, 127]);
    rig.send([144, 84, 0]);
    rig.wait("PFL on", |r| {
        r.audio.mixer_state().expect("mixer").channels[0].pfl
    });
    rig.wait("PFL LED", |r| r.led_seen([144, 84, 127]));

    rig.send([144, 78, 127]);
    rig.wait("vinyl off", |r| {
        !r.audio.deck_state(deck(0)).expect("state").vinyl_mode
    });
    rig.wait("vinyl LED off", |r| r.led_seen([144, 23, 0]));
}

#[test]
fn tempo_fader_picks_up_before_it_follows() {
    let mut rig = Rig::new("tempo");
    let fader = |r: &Rig| r.audio.deck_state(deck(0)).expect("state").tempo_fader;
    // The fader starts at one end while the software value is centred: ignored.
    rig.send([176, 0, 127]);
    rig.send([176, 32, 127]);
    std::thread::sleep(Duration::from_millis(100));
    rig.settle();
    assert!(fader(&rig).abs() < 1e-3, "ignored before pickup");
    // It crosses the software value: picked up, then followed.
    rig.send([176, 0, 64]);
    rig.send([176, 32, 0]);
    rig.send([176, 0, 100]);
    rig.send([176, 32, 0]);
    rig.wait("fader follows", |r| fader(r) > 0.4);
}

/// Every recorded FLX4 message of the midi crate, through the bridge: nothing
/// panics or hangs, and the engine still answers afterwards.
#[test]
fn every_recorded_fixture_passes_through_the_bridge() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../midi/tests/fixtures/flx4");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .collect();
    files.sort();
    assert!(files.len() >= 18);
    let mut rig = Rig::new("fixtures");
    let mut fed = 0;
    for file in &files {
        let text = std::fs::read_to_string(file).expect("fixture");
        for line in text.lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(bytes) = value.get("in").and_then(|v| v.as_array()) else {
                continue;
            };
            let bytes: Vec<u8> = bytes
                .iter()
                .filter_map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                .collect();
            if let [a, b, c] = bytes[..] {
                rig.send([a, b, c]);
                fed += 1;
            }
        }
    }
    assert!(fed > 300, "fed {fed} messages");
    assert!(rig.audio.mixer_state().is_ok());
    assert!(rig.audio.fx_state().is_ok());
}

#[test]
fn key_shift_pads_set_the_deck_key_and_light_the_active_pitch() {
    let mut rig = Rig::new("keyshift");
    // SAMPLER + SHIFT mode button of deck 1 selects KEY SHIFT (note 0x6F).
    rig.send([144, 111, 127]);
    rig.send([144, 111, 0]);
    assert_eq!(rig.core.pad_mode(deck(0)), PadMode::KeyShift);
    // Pad 6 of KEY SHIFT (note 0x75) is +1 semitone.
    rig.send([151, 117, 127]);
    rig.send([151, 117, 0]);
    rig.wait("key shift +1", |r| {
        r.audio.deck_state(deck(0)).expect("state").key_shift == 1
    });
    rig.wait("pitch pad lit", |r| r.led_seen([151, 117, 127]));
    // SHIFT + pad 7 raises the layout by one semitone: pad 6 is now +2.
    rig.send([152, 118, 127]);
    rig.send([152, 118, 0]);
    rig.send([151, 117, 127]);
    rig.wait("key shift +2", |r| {
        r.audio.deck_state(deck(0)).expect("state").key_shift == 2
    });
}

#[test]
fn ui_and_controller_mode_changes_share_one_state() {
    let mut rig = Rig::new("modesync");
    // A UI click selects SAMPLER; the controller's mode LED follows.
    assert!(rig.core.set_pad_mode(deck(0), PadMode::Sampler));
    rig.wait("SAMPLER LED", |r| r.led_seen([144, 34, 127]));
    // The HOT CUE button puts it back; the UI reads the same state.
    rig.send([144, 27, 127]);
    rig.send([144, 27, 0]);
    assert_eq!(rig.core.pad_mode(deck(0)), PadMode::HotCue);
    assert_eq!(
        rig.core.deck_state(deck(0)).expect("state").pad_mode,
        PadMode::HotCue
    );
}

#[test]
fn sampler_pads_light_when_a_sample_is_loaded() {
    let mut rig = Rig::new("sampler");
    let wav = tmp_dir("sampler-sample").join("kick.wav");
    write_wav(&wav, 0.5, 80.0);
    rig.core.set_pad_mode(deck(0), PadMode::Sampler);
    rig.core.sampler().load_path(0, &wav, None).expect("load");
    rig.wait("slot 1 loaded", |r| {
        r.core.sampler().state().slots[0].loaded
    });
    rig.wait("pad 1 lit", |r| r.led_seen([151, 48, 127]));
    assert!(!rig.led_seen([151, 49, 127]), "an empty slot stays dark");
    assert_eq!(rig.core.sampler().state().slots[0].name, "kick");
    // A pad press reaches the sampler without error; SHIFT + pad stops it.
    rig.send([151, 48, 127]);
    rig.send([151, 48, 0]);
    rig.send([152, 48, 127]);
    rig.send([152, 48, 0]);
    // Deck 2's first pad is slot 9 (empty), so it stays dark.
    rig.core.set_pad_mode(deck(1), PadMode::Sampler);
    rig.settle();
    assert!(!rig.led_seen([153, 48, 127]));
}
