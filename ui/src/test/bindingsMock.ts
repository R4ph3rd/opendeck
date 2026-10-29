import { vi } from "vitest";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

const channel = (deck: number) => ({
  deck,
  trim: 1,
  eq: [1, 1, 1],
  filter: 0,
  fader: 1,
  pfl: false,
  crossfader_assign: "thru",
});

/** Mixer as the backend reports it before anything is touched. */
export const neutralMixer = () => ({
  channels: [0, 1, 2, 3].map(channel),
  crossfader: 0,
  crossfader_curve: "constant_power",
  master_level: 1,
  headphone_level: 1,
  headphone_mix: 0,
  split_cue: false,
});

/** Deck state as the backend reports it before anything is touched. */
export const deckState = (deck: number) => ({
  deck,
  track: null,
  play_state: "empty",
  cue_point: null,
  track_bpm: null,
  track_key: null,
  beat_grid: null,
  tempo_range: "percent10",
  tempo_fader: 0,
  key_lock: false,
  key_shift: 0,
  sync: "off",
  is_leader: false,
  slip: false,
  quantize: true,
  vinyl_mode: true,
  active_loop: null,
  pad_mode: "hot_cue",
});

/**
 * Mock of the generated `ipc/bindings` module. Use inside `vi.mock` with
 * `vi.hoisted`:
 *
 *   const mock = vi.hoisted(() => createBindingsMock());
 *   vi.mock("../ipc/bindings", () => mock);
 */
export function createBindingsMock() {
  const listeners = new Map<string, (event: { payload: unknown }) => void>();
  const event = (name: string) => ({
    listen: vi.fn((cb: (event: { payload: unknown }) => void) => {
      listeners.set(name, cb);
      return Promise.resolve(() => listeners.delete(name));
    }),
  });
  return {
    commands: {
      appInfo: vi.fn(),
      listAudioHosts: vi.fn(() => ok([{ kind: "alsa", name: "ALSA", available: true, is_default: true }])),
      listAudioDevices: vi.fn(() =>
        ok([
          {
            name: "Test Device",
            is_default: true,
            max_channels: 4,
            default_sample_rate: 48_000,
            sample_rate_ranges: [[44_100, 96_000]],
            buffer_frames: [64, 2048],
          },
        ]),
      ),
      getAudioSettings: vi.fn(() =>
        ok({
          host: null,
          device: null,
          sample_rate: null,
          buffer_frames: null,
          master: { left: 0, right: 1 },
          headphones: { left: 2, right: 3 },
        }),
      ),
      setAudioSettings: vi.fn(() => ok({ running: true, device: "Test Device", sample_rate: 48_000 })),
      getAudioStatus: vi.fn(() => ok({ running: true, device: "Test Device", sample_rate: 48_000 })),
      supportedAudioExtensions: vi.fn(() => Promise.resolve(["wav", "mp3"])),
      loadTrack: vi.fn(() => ok(null)),
      ejectTrack: vi.fn(() => ok(null)),
      setPlaying: vi.fn(() => ok(null)),
      togglePlaying: vi.fn(() => ok(null)),
      cue: vi.fn(() => ok(null)),
      seek: vi.fn(() => ok(null)),
      setTempoFader: vi.fn(() => ok(null)),
      setTempoRange: vi.fn(() => ok(null)),
      setKeyLock: vi.fn(() => ok(null)),
      pitchBend: vi.fn(() => ok(null)),
      getPadMode: vi.fn(() => ok("hot_cue")),
      getDeckState: vi.fn((deck: number) => ok(deckState(deck))),
      setSync: vi.fn(() => ok(null)),
      setLeader: vi.fn(() => ok(null)),
      setQuantize: vi.fn(() => ok(null)),
      setSlip: vi.fn(() => ok(null)),
      setVinylMode: vi.fn(() => ok(null)),
      jogTouch: vi.fn(() => ok(null)),
      jogTurn: vi.fn(() => ok(null)),
      jogSearch: vi.fn(() => ok(null)),
      loopIn: vi.fn(() => ok(null)),
      loopOut: vi.fn(() => ok(null)),
      loopToggle: vi.fn(() => ok(null)),
      autoLoop: vi.fn(() => ok(null)),
      loopRoll: vi.fn(() => ok(null)),
      resizeLoop: vi.fn(() => ok(null)),
      beatJump: vi.fn(() => ok(null)),
      setHotCue: vi.fn(() => ok(null)),
      triggerHotCue: vi.fn(() => ok(null)),
      deleteHotCue: vi.fn(() => ok(null)),
      getHotCues: vi.fn(() => ok(null)),
      addMemoryCue: vi.fn(() => ok(null)),
      callMemoryCue: vi.fn(() => ok(null)),
      getMixerState: vi.fn(() => ok(neutralMixer())),
      setTrim: vi.fn(() => ok(null)),
      setEq: vi.fn(() => ok(null)),
      setFilter: vi.fn(() => ok(null)),
      setChannelFader: vi.fn(() => ok(null)),
      setPfl: vi.fn(() => ok(null)),
      setCrossfader: vi.fn(() => ok(null)),
      setCrossfaderCurve: vi.fn(() => ok(null)),
      setCrossfaderAssign: vi.fn(() => ok(null)),
      setMasterLevel: vi.fn(() => ok(null)),
      setHeadphoneLevel: vi.fn(() => ok(null)),
      setHeadphoneMix: vi.fn(() => ok(null)),
      setSplitCue: vi.fn(() => ok(null)),
    },
    events: {
      audioDeviceChanged: event("audio-device-changed"),
      engineEvent: event("engine-event"),
      engineTelemetry: event("engine-telemetry"),
      hotCuesChanged: event("hot-cues-changed"),
      trackLoadFailed: event("track-load-failed"),
      trackLoaded: event("track-loaded"),
    },
    /** Delivers `payload` to the listener registered for `name`. */
    emit(name: string, payload: unknown) {
      listeners.get(name)?.({ payload });
    },
  };
}
