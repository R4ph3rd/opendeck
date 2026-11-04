/**
 * Decoder for the waveform peak blob, `contracts::peaks` v1 (ADR 0004):
 * 24-byte little-endian header, then `peak, low, mid, high` as u8 per point.
 */

export const PEAK_MAGIC = "ODPK";
export const PEAK_FORMAT_VERSION = 1;
export const PEAK_HEADER_LEN = 24;
export const PEAK_POINT_LEN = 4;

export type PeakDecodeCode = "truncated" | "bad_magic" | "unsupported_version" | "invalid";

export class PeakDecodeError extends Error {
  readonly code: PeakDecodeCode;
  constructor(code: PeakDecodeCode, message: string) {
    super(message);
    this.name = "PeakDecodeError";
    this.code = code;
  }
}

/** Peaks of one zoom level. `points` holds `count * 4` bytes: peak, low, mid, high. */
export interface PeakLevelData {
  /** Rate of the analysed audio, Hz. */
  sampleRate: number;
  /** Frames summarised by each point. */
  framesPerPoint: number;
  count: number;
  points: Uint8Array;
}

/** One column of band levels, `0..=255`. */
export interface PeakSample {
  peak: number;
  low: number;
  mid: number;
  high: number;
}

export function decodePeaks(input: ArrayBuffer | ArrayBufferView): PeakLevelData {
  const bytes =
    input instanceof ArrayBuffer
      ? new Uint8Array(input)
      : new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  if (bytes.length < PEAK_HEADER_LEN) {
    throw new PeakDecodeError("truncated", "peak blob shorter than its header");
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  for (let i = 0; i < PEAK_MAGIC.length; i += 1) {
    if (bytes[i] !== PEAK_MAGIC.charCodeAt(i)) {
      throw new PeakDecodeError("bad_magic", "not a peak blob");
    }
  }
  const version = view.getUint16(4, true);
  const pointLen = view.getUint16(6, true);
  if (version !== PEAK_FORMAT_VERSION || pointLen !== PEAK_POINT_LEN) {
    throw new PeakDecodeError("unsupported_version", `unsupported peak format version ${version}`);
  }
  const sampleRate = view.getUint32(8, true);
  const framesPerPoint = view.getUint32(12, true);
  const count = view.getUint32(16, true);
  if (sampleRate === 0 || framesPerPoint === 0) {
    throw new PeakDecodeError("invalid", "invalid peak blob header");
  }
  if (bytes.length - PEAK_HEADER_LEN < count * PEAK_POINT_LEN) {
    throw new PeakDecodeError("truncated", "peak blob shorter than its point count");
  }
  return {
    sampleRate,
    framesPerPoint,
    count,
    points: bytes.subarray(PEAK_HEADER_LEN, PEAK_HEADER_LEN + count * PEAK_POINT_LEN),
  };
}

/** Encodes a level in the v1 layout. Used by tests and Storybook mocks. */
export function encodePeaks(level: PeakLevelData): Uint8Array {
  const out = new Uint8Array(PEAK_HEADER_LEN + level.count * PEAK_POINT_LEN);
  const view = new DataView(out.buffer);
  for (let i = 0; i < PEAK_MAGIC.length; i += 1) out[i] = PEAK_MAGIC.charCodeAt(i);
  view.setUint16(4, PEAK_FORMAT_VERSION, true);
  view.setUint16(6, PEAK_POINT_LEN, true);
  view.setUint32(8, level.sampleRate, true);
  view.setUint32(12, level.framesPerPoint, true);
  view.setUint32(16, level.count, true);
  out.set(level.points.subarray(0, level.count * PEAK_POINT_LEN), PEAK_HEADER_LEN);
  return out;
}

/** Duration the level covers, seconds. */
export function levelSeconds(level: PeakLevelData): number {
  return (level.count * level.framesPerPoint) / level.sampleRate;
}

/**
 * Picks the coarsest level whose points are no wider than a pixel column, so
 * every column aggregates at least one point. Falls back to the finest.
 * `levels` may contain nulls (not loaded yet) and any order.
 */
export function selectLevel(
  levels: readonly (PeakLevelData | null)[],
  framesPerPixel: number,
): PeakLevelData | null {
  let best: PeakLevelData | null = null;
  let finest: PeakLevelData | null = null;
  for (const level of levels) {
    if (!level) continue;
    if (!finest || level.framesPerPoint < finest.framesPerPoint) finest = level;
    if (
      level.framesPerPoint <= framesPerPixel &&
      (!best || level.framesPerPoint > best.framesPerPoint)
    ) {
      best = level;
    }
  }
  return best ?? finest;
}

/**
 * Band levels over the frame range `[startFrame, endFrame)` of the level's own
 * sample rate. A range spanning several points takes the max of each band. A
 * range narrower than one point takes that point as is: no interpolation, so
 * every column shows real data. Outside the track the result is zero.
 */
export function sampleRange(
  level: PeakLevelData,
  startFrame: number,
  endFrame: number,
  out: PeakSample,
): PeakSample {
  const fpp = level.framesPerPoint;
  const p = level.points;
  const first = Math.floor(startFrame / fpp);
  const last = Math.ceil(endFrame / fpp) - 1;
  if (last < 0 || first >= level.count) {
    out.peak = out.low = out.mid = out.high = 0;
    return out;
  }
  if (endFrame - startFrame >= fpp) {
    const a = Math.max(0, first);
    const b = Math.min(level.count - 1, last);
    let peak = 0;
    let low = 0;
    let mid = 0;
    let high = 0;
    for (let i = a; i <= b; i += 1) {
      const o = i * PEAK_POINT_LEN;
      const vp = p[o] ?? 0;
      const vl = p[o + 1] ?? 0;
      const vm = p[o + 2] ?? 0;
      const vh = p[o + 3] ?? 0;
      if (vp > peak) peak = vp;
      if (vl > low) low = vl;
      if (vm > mid) mid = vm;
      if (vh > high) high = vh;
    }
    out.peak = peak;
    out.low = low;
    out.mid = mid;
    out.high = high;
    return out;
  }
  // Narrower than a point: take that single point. Never interpolate.
  const idx = Math.min(level.count - 1, Math.max(0, Math.floor((startFrame + endFrame) / 2 / fpp)));
  const o = idx * PEAK_POINT_LEN;
  out.peak = p[o] ?? 0;
  out.low = p[o + 1] ?? 0;
  out.mid = p[o + 2] ?? 0;
  out.high = p[o + 3] ?? 0;
  return out;
}

/**
 * Fills `out` (4 bytes per column: peak, low, mid, high) for `columns` device
 * pixel columns. Column `x` covers world pixels `[x - centre + offset, +1)` at
 * `pps` device pixels per second; each column takes the max of the points it
 * touches in the given level and never blends neighbours.
 */
export function aggregateColumns(
  level: PeakLevelData,
  columns: number,
  centre: number,
  offset: number,
  pps: number,
  out: Uint8Array,
  scratch: PeakSample,
): void {
  const sr = level.sampleRate;
  for (let x = 0; x < columns; x += 1) {
    const c = x - centre + offset;
    sampleRange(level, (c / pps) * sr, ((c + 1) / pps) * sr, scratch);
    const o = x * PEAK_POINT_LEN;
    out[o] = scratch.peak;
    out[o + 1] = scratch.low;
    out[o + 2] = scratch.mid;
    out[o + 3] = scratch.high;
  }
}
