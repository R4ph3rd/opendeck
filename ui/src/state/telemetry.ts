import { createStore } from "zustand/vanilla";
import type { DeckTelemetry, EngineTelemetry } from "../ipc/bindings";

/**
 * Engine telemetry arrives at ~60 Hz. It lives in a vanilla store that React
 * never subscribes to as a whole: animation-frame loops read `getState()`, and
 * components select coarse slices (a boolean, a deck flag) so a tick only
 * re-renders when that slice changes.
 */
export interface TelemetryState {
  /** Newest engine snapshot, `null` until the first one arrives. */
  telemetry: EngineTelemetry | null;
  /** Stores a snapshot. Older sequence numbers are ignored. */
  push: (snapshot: EngineTelemetry) => void;
  /** Forgets everything (engine restart, tests). */
  reset: () => void;
}

export const telemetryStore = createStore<TelemetryState>()((set, get) => ({
  telemetry: null,
  push: (snapshot) => {
    const current = get().telemetry;
    if (current && snapshot.sequence < current.sequence) {
      return;
    }
    set({ telemetry: snapshot });
  },
  reset: () => set({ telemetry: null }),
}));

/** Telemetry of one deck, `null` before the first snapshot or for an unknown deck. */
export function selectDeck(state: TelemetryState, deck: number): DeckTelemetry | null {
  return state.telemetry?.decks[deck] ?? null;
}

/** Seconds for an engine frame count at the snapshot's sample rate. */
export function framesToSeconds(frames: number, sampleRate: number): number {
  return sampleRate > 0 ? frames / sampleRate : 0;
}
