import { vi } from "vitest";

const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

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
    },
    events: {
      audioDeviceChanged: event("audio-device-changed"),
      engineEvent: event("engine-event"),
      engineTelemetry: event("engine-telemetry"),
      trackLoadFailed: event("track-load-failed"),
      trackLoaded: event("track-loaded"),
    },
    /** Delivers `payload` to the listener registered for `name`. */
    emit(name: string, payload: unknown) {
      listeners.get(name)?.({ payload });
    },
  };
}
