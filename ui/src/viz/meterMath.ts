import { METER_FLOOR_DB } from "../state/telemetry";

/** Fill fraction where the bar turns amber (-12 dBFS). */
export const AMBER_FROM = (METER_FLOOR_DB + 12) / METER_FLOOR_DB;
/** Fill fraction where the bar turns red (-3 dBFS). */
export const RED_FROM = (METER_FLOOR_DB + 3) / METER_FLOOR_DB;

/** Peak hold time before the marker falls, ms. */
export const HOLD_MS = 1200;
/** Fall speed of the hold marker afterwards, fraction of the scale per second. */
export const HOLD_FALL_PER_S = 0.6;
/** A clip indicator stays lit this long after the last clipped frame, ms. */
export const CLIP_HOLD_MS = 3000;

/** Fraction `0..1` for dBFS on the meter scale. Mirrors `meterFraction` for a dB input. */
export function dbToFraction(db: number): number {
  return Math.min(1, Math.max(0, (db - METER_FLOOR_DB) / -METER_FLOOR_DB));
}

export interface HoldState {
  /** Held fraction. */
  value: number;
  /** When `value` was last raised or held, ms. */
  at: number;
  /** Last frame time, for the fall. */
  lastNow: number;
}

export const newHold = (): HoldState => ({ value: 0, at: 0, lastNow: 0 });

/** Updates the hold marker for a new level. A new peak resets the hold timer. */
export function updateHold(state: HoldState, level: number, now: number): number {
  if (level >= state.value) {
    state.value = level;
    state.at = now;
  } else if (now - state.at > HOLD_MS) {
    const dt = Math.max(0, now - Math.max(state.lastNow, state.at + HOLD_MS)) / 1000;
    state.value = Math.max(level, state.value - dt * HOLD_FALL_PER_S);
  }
  state.lastNow = now;
  return state.value;
}

/** Tracks a clip latch. Returns whether the indicator is lit at `now`. */
export function updateClip(latchedAt: { t: number }, clipping: boolean, now: number): boolean {
  if (clipping) {
    latchedAt.t = now;
    return true;
  }
  return latchedAt.t > 0 && now - latchedAt.t < CLIP_HOLD_MS;
}
