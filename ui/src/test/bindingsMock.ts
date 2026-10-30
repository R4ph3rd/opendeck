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

const key = (id: string) => ({ provider: "builtin", id });

const param = (
  id: number,
  name: string,
  unit: string,
  min: number,
  max: number,
  def: number,
  curve: unknown = { kind: "linear" },
) => ({ id, name, unit, min, max, default: def, curve });

/** A few catalog effects as `fx_list_effects` reports them. */
export const fxEffects = () => [
  {
    key: key("echo"),
    name: "Echo",
    routing: "send_return",
    beat_synced: true,
    has_tail: true,
    params: [
      param(0, "Time", "milliseconds", 10, 4000, 500, { kind: "logarithmic" }),
      param(1, "Decay", "percent", 0, 98, 55),
      param(2, "Damping", "hertz", 800, 16000, 5000, { kind: "logarithmic" }),
    ],
    primary: 1,
    release: false,
    divisions: [0.0625, 0.125, 0.25, 0.5, 0.75, 1, 2, 4],
  },
  {
    key: key("reverb"),
    name: "Reverb",
    routing: "send_return",
    beat_synced: false,
    has_tail: true,
    params: [param(0, "Size", "percent", 0, 100, 50)],
    primary: 0,
    release: false,
    divisions: [],
  },
  {
    key: key("vinyl_brake"),
    name: "Vinyl Brake",
    routing: "insert",
    beat_synced: true,
    has_tail: false,
    params: [param(0, "Curve", "percent", 0, 100, 40)],
    primary: 0,
    release: true,
    divisions: [0.25, 0.5, 0.75, 1, 2, 4],
  },
  {
    key: key("colour_filter"),
    name: "Colour Filter",
    routing: "insert",
    beat_synced: false,
    has_tail: false,
    params: [param(0, "Colour", "percent", -100, 100, 0)],
    primary: 0,
    release: false,
    divisions: [],
  },
  {
    key: key("noise"),
    name: "Noise",
    routing: "insert",
    beat_synced: false,
    has_tail: false,
    params: [param(0, "Level", "percent", 0, 100, 30)],
    primary: 0,
    release: false,
    divisions: [],
  },
];

const fxSlot = (effect: ReturnType<typeof key> | null, params: number[], division = 1) => ({
  effect,
  params,
  wet: 0.5,
  division,
  enabled: true,
});

const emptySlot = () => fxSlot(null, []);

const beatUnit = (assign: string, effect: string, params: number[], division: number) => ({
  assign,
  mode: "multi",
  slots: [fxSlot(key(effect), params, division), emptySlot(), emptySlot()],
  on: false,
  bpm_source: "auto",
  tap_bpm: null as number | null,
  release_effect: key("vinyl_brake"),
  release_hold_ms: 2000,
});

/** FX model as the backend reports it before anything is touched. */
export const neutralFxState = () => ({
  units: [
    beatUnit("ch1", "echo", [0.1, 0.5, 0.5], 0.75),
    beatUnit("ch2", "reverb", [0.5], 1),
  ],
  cfx: [0, 1].map(() => ({ kind: key("colour_filter"), amount: 0 })),
  cfx_kinds: [key("colour_filter"), key("noise")],
  sends: [0, 1].map(() => ({
    slots: [fxSlot(key("reverb"), [0.5]), emptySlot(), emptySlot()],
    post_fader: true,
  })),
  send_levels: [
    [0, 0],
    [0, 0],
  ],
  pads: Array.from({ length: 32 }, (_, i) => ({
    effect: i % 2 === 0 ? key("echo") : key("noise"),
    division: 0.5,
    release: false,
  })),
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
      loadTrackPath: vi.fn(() => ok(null)),
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
      queryTracks: vi.fn(() => ok({ rows: [], total: 0 })),
      getTrack: vi.fn(() => ok(null)),
      updateTrackMeta: vi.fn(() => ok(null)),
      setRating: vi.fn(() => ok(null)),
      deleteTracks: vi.fn((tracks: number[]) => ok(tracks.length)),
      getArtwork: vi.fn(() => ok([])),
      getTrackPeaks: vi.fn(() => ok(new ArrayBuffer(0))),
      getAnalysis: vi.fn(() => ok(null)),
      setBeatGrid: vi.fn(() => ok(null)),
      listCues: vi.fn(() => ok([])),
      importFolder: vi.fn(() => ok(1)),
      importFiles: vi.fn(() => ok(1)),
      importPlaylistFile: vi.fn(() => ok(1)),
      importRekordboxXml: vi.fn(() => ok(1)),
      reanalyzeTracks: vi.fn(() => ok(1)),
      listJobs: vi.fn(() => ok([])),
      pauseJob: vi.fn(() => ok(null)),
      resumeJob: vi.fn(() => ok(null)),
      cancelJob: vi.fn(() => ok(null)),
      listPlaylists: vi.fn(() => ok([])),
      createPlaylist: vi.fn(() => ok(1)),
      renamePlaylist: vi.fn(() => ok(null)),
      deletePlaylist: vi.fn(() => ok(null)),
      movePlaylist: vi.fn(() => ok(null)),
      getPlaylistTracks: vi.fn(() => ok([])),
      addToPlaylist: vi.fn(() => ok(null)),
      removeFromPlaylist: vi.fn(() => ok(null)),
      reorderPlaylist: vi.fn(() => ok(null)),
      listSavedSets: vi.fn(() => ok([])),
      getSavedSet: vi.fn(() => ok(null)),
      renameSavedSet: vi.fn(() => ok(null)),
      deleteSavedSet: vi.fn(() => ok(null)),
      fxGetState: vi.fn(() => ok(neutralFxState())),
      fxListEffects: vi.fn(() => ok(fxEffects())),
      fxGetPresets: vi.fn(() => ok({ factory: [], user: [], chains: [] })),
      fxApply: vi.fn(() => ok(null)),
      fxPad: vi.fn(() => ok(null)),
      fxSavePreset: vi.fn(() => ok({ factory: [], user: [], chains: [] })),
      fxDeletePreset: vi.fn(() => ok({ factory: [], user: [], chains: [] })),
      fxSaveChain: vi.fn(() => ok({ factory: [], user: [], chains: [] })),
      fxDeleteChain: vi.fn(() => ok({ factory: [], user: [], chains: [] })),
      listDirectory: vi.fn(() => ok({ path: "/music", parent: "/", entries: [] })),
      revealInExplorer: vi.fn(() => ok(null)),
    },
    events: {
      analysisReady: event("analysis-ready"),
      audioDeviceChanged: event("audio-device-changed"),
      cuesChanged: event("cues-changed"),
      importProgress: event("import-progress"),
      libraryChanged: event("library-changed"),
      engineEvent: event("engine-event"),
      engineTelemetry: event("engine-telemetry"),
      hotCuesChanged: event("hot-cues-changed"),
      fxStateChanged: event("fx-state-changed"),
      trackLoadFailed: event("track-load-failed"),
      trackLoaded: event("track-loaded"),
    },
    /** Delivers `payload` to the listener registered for `name`. */
    emit(name: string, payload: unknown) {
      listeners.get(name)?.({ payload });
    },
  };
}
