import { createStore } from "zustand/vanilla";
import type { DeckTelemetry, EngineTelemetry, SyncMode } from "../ipc/bindings";

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

/** Quietest level a meter shows, in dBFS. Anything below reads as empty. */
export const METER_FLOOR_DB = -60;

/** Meter fill `0..1` for a linear peak: dBFS from the floor to full scale. */
export function meterFraction(peak: number | null): number {
  if (peak === null || !(peak > 0)) {
    return 0;
  }
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db - METER_FLOOR_DB) / -METER_FLOOR_DB));
}

/** A peak at or above full scale is a clip. */
export const isClipping = (peak: number | null): boolean => peak !== null && peak >= 1;

/** The deck is looping. Boolean, so a component using it re-renders only on change. */
export function selectLoopActive(state: TelemetryState, deck: number): boolean {
  return state.telemetry?.decks[deck]?.loop_active ?? false;
}

/** Sync mode the engine applies to a deck, `null` before the first snapshot. */
export function selectSyncMode(state: TelemetryState, deck: number): SyncMode | null {
  return state.telemetry?.decks[deck]?.sync_mode ?? null;
}

/** The engine reports this deck as the sync leader. */
export function selectIsLeader(state: TelemetryState, deck: number): boolean {
  return state.telemetry?.decks[deck]?.is_leader ?? false;
}

/** Sync leader the engine reports, `null` before the first snapshot or with no leader. */
export function selectLeader(state: TelemetryState): number | null {
  return state.telemetry?.leader ?? null;
}

/** Effective BPM of a deck, `null` until analysis (M3) or a grid provides one. */
export function selectBpm(state: TelemetryState, deck: number): number | null {
  return state.telemetry?.decks[deck]?.bpm ?? null;
}
