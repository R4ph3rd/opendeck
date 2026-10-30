import type { AudioStatus, DeckTelemetry, EngineTelemetry, TrackInfo } from "../ipc/bindings";

export const RATE = 48_000;

const meter = { peak_l: 0, peak_r: 0, rms_l: 0, rms_r: 0 };

export function deckTelemetry(patch: Partial<DeckTelemetry> = {}): DeckTelemetry {
  return {
    loaded: false,
    playing: false,
    position: 0,
    duration: 0,
    slip_position: null,
    rate: 1,
    bpm: null,
    beat_phase: 0,
    beat_in_bar: 0,
    sync_mode: "off",
    is_leader: false,
    loop_region: null,
    loop_active: false,
    meter,
    ...patch,
  };
}

/** Snapshot with deck A patched and the other decks empty. */
export function telemetry(sequence: number, deckA: Partial<DeckTelemetry> = {}): EngineTelemetry {
  return {
    sequence,
    engine_time: 0,
    sample_rate: RATE,
    decks: [deckTelemetry(deckA), deckTelemetry(), deckTelemetry(), deckTelemetry()],
    master: meter,
    headphones: meter,
    leader: null,
    dsp_load: 0,
    xruns: 0,
    dropped_events: 0,
  };
}

export const track: TrackInfo = {
  path: "/music/tone.wav",
  file_name: "tone.wav",
  title: "Night Drive",
  artist: "Test Artist",
  album: null,
  duration_secs: 200,
  id: 7,
  bpm: 126,
  key: "8A",
  key_musical: "Am",
};

export const runningAudio: AudioStatus = {
  running: true,
  backend: "device",
  host: "alsa",
  device: "Test Device",
  sample_rate: RATE,
  channels: 4,
  buffer_frames: 256,
  latency_ms: 5.3,
  master: { left: 0, right: 1 },
  headphones: { left: 2, right: 3 },
  stereo_fallback: false,
  error: null,
};

/** Snapshot with `deck` patched and the others empty; `leader` is the reported sync leader. */
export function telemetryFor(
  sequence: number,
  deck: number,
  patch: Partial<DeckTelemetry> = {},
  leader: number | null = null,
): EngineTelemetry {
  const base = telemetry(sequence);
  return {
    ...base,
    leader,
    decks: base.decks.map((d, i) => ({
      ...(i === deck ? deckTelemetry(patch) : d),
      is_leader: i === leader,
    })) as EngineTelemetry["decks"],
  };
}
