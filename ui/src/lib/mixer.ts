/**
 * Knob and level curves. Knobs hold a normalised position; the backend takes
 * linear gain (docs/ipc.md). The ranges mirror `contracts::mixer`
 * (`EQ_MAX_GAIN_DB`, `TRIM_MAX_GAIN_DB`); the engine clamps whatever arrives.
 */

/** Highest EQ boost in dB (`contracts::mixer::EQ_MAX_GAIN_DB`). */
export const EQ_MAX_GAIN_DB = 6;
/** Trim range is plus or minus this many dB (`contracts::mixer::TRIM_MAX_GAIN_DB`). */
export const TRIM_MAX_GAIN_DB = 12;
/** EQ cut at the first step above the kill position. UI taper, not from a spec. */
export const EQ_FLOOR_DB = -40;
/** Level knob cut at the first step above silence. UI taper, not from a spec. */
export const LEVEL_FLOOR_DB = -48;
/** Level knob position that gives unity gain; above it the knob boosts. */
export const LEVEL_UNITY_POSITION = 0.75;
/** Level knob boost at full travel in dB. */
export const LEVEL_MAX_GAIN_DB = 6;

/** Default positions. */
export const TRIM_DEFAULT = 0.5;
export const EQ_DEFAULT = 0.5;
export const LEVEL_DEFAULT = LEVEL_UNITY_POSITION;
export const FILTER_DEFAULT = 0;
export const FADER_DEFAULT = 1;
export const CROSSFADER_DEFAULT = 0;
export const HEADPHONE_MIX_DEFAULT = 0;

const clamp01 = (value: number): number => Math.min(1, Math.max(0, value));

export function dbToGain(db: number): number {
  return 10 ** (db / 20);
}

/** dB of a linear gain; `-Infinity` for silence. */
export function gainToDb(gain: number): number {
  return gain > 0 ? 20 * Math.log10(gain) : Number.NEGATIVE_INFINITY;
}

/** Trim knob `0..1` to linear gain; the middle is unity. */
export function trimGain(position: number): number {
  return dbToGain((clamp01(position) * 2 - 1) * TRIM_MAX_GAIN_DB);
}

export function trimPosition(gain: number): number {
  const db = Number.isFinite(gainToDb(gain)) ? gainToDb(gain) : -TRIM_MAX_GAIN_DB;
  return clamp01((db / TRIM_MAX_GAIN_DB + 1) / 2);
}

/** EQ knob `0..1` to linear gain: `0` kills the band, the middle is unity. */
export function eqGain(position: number): number {
  const p = clamp01(position);
  if (p === 0) {
    return 0;
  }
  if (p <= 0.5) {
    return dbToGain(EQ_FLOOR_DB * (1 - p / 0.5));
  }
  return dbToGain(((p - 0.5) / 0.5) * EQ_MAX_GAIN_DB);
}

export function eqPosition(gain: number): number {
  if (!(gain > 0)) {
    return 0;
  }
  const db = gainToDb(gain);
  if (db <= 0) {
    return clamp01(0.5 * (1 - db / EQ_FLOOR_DB));
  }
  return clamp01(0.5 + (db / EQ_MAX_GAIN_DB) * 0.5);
}

/** Master and headphone level knob `0..1` to linear gain. */
export function levelGain(position: number): number {
  const p = clamp01(position);
  if (p === 0) {
    return 0;
  }
  if (p <= LEVEL_UNITY_POSITION) {
    return dbToGain(LEVEL_FLOOR_DB * (1 - p / LEVEL_UNITY_POSITION));
  }
  return dbToGain(((p - LEVEL_UNITY_POSITION) / (1 - LEVEL_UNITY_POSITION)) * LEVEL_MAX_GAIN_DB);
}

export function levelPosition(gain: number): number {
  if (!(gain > 0)) {
    return 0;
  }
  const db = gainToDb(gain);
  if (db <= 0) {
    return clamp01(LEVEL_UNITY_POSITION * (1 - db / LEVEL_FLOOR_DB));
  }
  return clamp01(
    LEVEL_UNITY_POSITION + (db / LEVEL_MAX_GAIN_DB) * (1 - LEVEL_UNITY_POSITION),
  );
}

function formatDb(db: number): string {
  if (!Number.isFinite(db)) {
    return "off";
  }
  const rounded = Math.round(db * 10) / 10;
  return `${rounded > 0 ? "+" : ""}${rounded.toFixed(1)} dB`;
}

export const formatTrim = (position: number): string => formatDb(gainToDb(trimGain(position)));

export function formatEq(position: number): string {
  return position <= 0 ? "kill" : formatDb(gainToDb(eqGain(position)));
}

export function formatLevel(position: number): string {
  return position <= 0 ? "off" : formatDb(gainToDb(levelGain(position)));
}

/** Filter knob: negative low-pass, positive high-pass. */
export function formatFilter(amount: number): string {
  if (Math.abs(amount) < 0.005) {
    return "off";
  }
  return `${amount < 0 ? "LP" : "HP"} ${Math.round(Math.abs(amount) * 100)}%`;
}

export const formatPercent = (position: number): string => `${Math.round(position * 100)}%`;
