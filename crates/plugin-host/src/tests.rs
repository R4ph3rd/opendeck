//! Host tests against the fake format, with a dedicated main thread.

use std::path::Path;
use std::sync::Arc;

use opendeck_contracts::fx::{EffectKey, EffectRegistry, ParamId, ProcessContext, StereoBufferMut};
use opendeck_contracts::instrument::{InstrumentRegistry, NoteEvent};
use opendeck_contracts::{Beats, SampleRate};

use crate::backend::ParamChange;
use crate::fake::{FAKE_GAIN, FAKE_PROVIDER, FAKE_SINE, FakeBackend};
use crate::main_thread::ThreadMainThread;
use crate::{InstanceStatus, PluginError, PluginHost};

#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn host() -> PluginHost {
    let main = Arc::new(ThreadMainThread::new("plugin-main").expect("thread"));
    let host = PluginHost::new(main, vec![Arc::new(FakeBackend)]);
    host.set_catalog(FakeBackend::plugins_in(Path::new("/x/a.fakeplug")));
    host
}

fn key(id: &str) -> EffectKey {
    EffectKey {
        provider: FAKE_PROVIDER.into(),
        id: id.into(),
    }
}

fn ctx() -> ProcessContext {
    ProcessContext {
        sample_rate: SampleRate(48_000),
        bpm: None,
        beat_position: None,
        beat_division: Beats(1.0),
        playing: true,
    }
}

#[test]
fn effect_processes_without_allocating() {
    let host = host();
    let (mut fx, handle) = host.create_effect(&key(FAKE_GAIN)).expect("create");
    assert_eq!(handle.status(), Ok(InstanceStatus::Inactive));
    assert_eq!(fx.info().params.len(), 1);
    fx.prepare(SampleRate(48_000), 64);
    assert_eq!(handle.status(), Ok(InstanceStatus::Active));
    let mut l = [1.0_f32; 64];
    let mut r = [1.0_f32; 64];
    let c = ctx();
    assert_no_alloc::assert_no_alloc(|| {
        fx.set_parameter(ParamId(0), 0.25);
        fx.set_parameter(ParamId(0), f32::NAN);
        let io = StereoBufferMut::new(&mut l, &mut r).expect("same len");
        fx.process(io, &c);
    });
    assert!(l.iter().chain(&r).all(|s| (*s - 0.5).abs() < 1e-6));
    assert_eq!(fx.latency_frames(), 0);
}

#[test]
fn state_round_trips_and_editor_edits_reach_audio() {
    let host = host();
    let (mut fx, handle) = host.create_effect(&key(FAKE_GAIN)).expect("create");
    fx.prepare(SampleRate(44_100), 8);
    let saved = handle.save_state().expect("save");
    handle
        .send_param(ParamChange {
            id: ParamId(0),
            normalized: 1.0,
            offset: 0,
        })
        .expect("send");
    let mut l = [1.0_f32; 8];
    let mut r = [1.0_f32; 8];
    fx.process(StereoBufferMut::new(&mut l, &mut r).expect("len"), &ctx());
    assert_eq!(l, [2.0; 8]);
    assert_ne!(handle.save_state().expect("save"), saved);
    handle.load_state(saved.clone()).expect("load");
    assert_eq!(handle.save_state().expect("save"), saved);
    assert!(handle.drain_param_output().expect("drain").is_empty());
}

#[test]
fn dropping_the_effect_destroys_the_instance() {
    let host = host();
    let (mut fx, handle) = host.create_effect(&key(FAKE_GAIN)).expect("create");
    fx.prepare(SampleRate(48_000), 16);
    drop(fx);
    assert_eq!(
        handle.status(),
        Err(PluginError::UnknownInstance(handle.id().0))
    );
}

#[test]
fn unprepared_effect_passes_through() {
    let host = host();
    let (mut fx, _) = host.create_effect(&key(FAKE_GAIN)).expect("create");
    let mut l = [0.3_f32; 4];
    let mut r = [0.3_f32; 4];
    fx.process(StereoBufferMut::new(&mut l, &mut r).expect("len"), &ctx());
    assert_eq!(l, [0.3; 4]);
}

#[test]
fn instrument_plays_notes() {
    let host = host();
    let (mut inst, _) = host.create_instrument(&key(FAKE_SINE)).expect("create");
    inst.prepare(SampleRate(48_000), 480);
    let mut l = [9.0_f32; 480];
    let mut r = [9.0_f32; 480];
    let on = [NoteEvent::on(0, 0, 69, 1.0).expect("note")];
    assert_no_alloc::assert_no_alloc(|| {
        inst.process(
            StereoBufferMut::new(&mut l, &mut r).expect("len"),
            &on,
            &ctx(),
        );
    });
    // 440 Hz at 48 kHz: 4.4 periods in 480 frames, peak near 1.
    let peak = l.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.99 && peak <= 1.0, "peak {peak}");
    let crossings = l.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
    assert!((4..=5).contains(&crossings), "crossings {crossings}");
    let off = [NoteEvent::off(0, 0, 69, 0.0).expect("note")];
    inst.process(
        StereoBufferMut::new(&mut l, &mut r).expect("len"),
        &off,
        &ctx(),
    );
    assert!(l.iter().chain(&r).all(|s| *s == 0.0));
}

#[test]
fn factories_list_by_kind_and_create() {
    let host = host();
    let mut effects = EffectRegistry::new();
    for f in host.effect_factories() {
        effects.register(f).expect("register");
    }
    let mut instruments = InstrumentRegistry::new();
    for f in host.instrument_factories() {
        instruments.register(f).expect("register");
    }
    let fx = effects.effects();
    assert_eq!(fx.len(), 1);
    assert_eq!(fx[0].key, key(FAKE_GAIN));
    assert!(fx[0].has_tail);
    assert_eq!(instruments.instruments()[0].key, key(FAKE_SINE));
    assert!(effects.create(&key(FAKE_GAIN)).is_ok());
    assert!(effects.create(&key(FAKE_SINE)).is_err());
    assert!(instruments.create(&key(FAKE_SINE)).is_ok());
}

#[test]
fn unknown_provider_is_reported() {
    let host = host();
    let k = EffectKey {
        provider: "clap".into(),
        id: "x".into(),
    };
    assert!(matches!(
        host.create_effect(&k),
        Err(PluginError::UnknownPlugin { .. })
    ));
}
