import type { BeatGrid } from "../ipc/bindings";

/** Visible window widths of the zoomed waveform, seconds. */
export const ZOOM_SECONDS = [2, 4, 8, 16, 32, 64] as const;
export const DEFAULT_ZOOM_INDEX = 2;

export function clampZoom(index: number): number {
  return Math.min(ZOOM_SECONDS.length - 1, Math.max(0, Math.round(index)));
}

/** Zoom index after `direction` steps: `+1` zooms in (narrower window). */
export function stepZoom(index: number, direction: 1 | -1): number {
  return clampZoom(index - direction);
}

/** Device pixels per second of track time for a window of `visibleSeconds` across `width`. */
export function pixelsPerSecond(width: number, visibleSeconds: number): number {
  return visibleSeconds > 0 ? width / visibleSeconds : 0;
}

/**
 * Device x of a track time. The playhead sits at `width / 2`. The centre offset
 * is rounded once so every element (columns, beats, cues) shares the same
 * rounding and nothing shimmers against the waveform while it scrolls.
 */
export function timeToX(time: number, playhead: number, width: number, pps: number): number {
  return Math.round(width / 2) + Math.round(time * pps) - Math.round(playhead * pps);
}

/** Track time at device x, the inverse of `timeToX` up to rounding. */
export function xToTime(x: number, playhead: number, width: number, pps: number): number {
  return pps > 0 ? playhead + (x - width / 2) / pps : playhead;
}

/** Seconds to move the playhead for a drag of `dxCss` CSS pixels. Dragging right pulls earlier audio under the playhead. */
export function scrubDelta(dxCss: number, cssWidth: number, visibleSeconds: number): number {
  return cssWidth > 0 ? (-dxCss / cssWidth) * visibleSeconds : 0;
}

export interface BeatLine {
  time: number;
  /** First beat of a bar. */
  downbeat: boolean;
}

/** Beats closer than this, in device pixels, are not drawn. */
export const MIN_BEAT_SPACING_PX = 4;

/**
 * Beat lines of a grid inside `[from, to]` seconds. The anchor is beat 0 and a
 * downbeat; grids extend before the anchor. Returns an empty list when beats
 * would be denser than `MIN_BEAT_SPACING_PX` at `pps`. Bars are skipped
 * (only downbeats) before beats get that dense.
 */
export function beatLines(
  grid: BeatGrid,
  from: number,
  to: number,
  pps: number,
  out: BeatLine[] = [],
): BeatLine[] {
  out.length = 0;
  const bpm = grid.bpm;
  const anchor = grid.anchor;
  const perBar = Math.max(1, Math.floor(grid.beats_per_bar));
  if (bpm === null || anchor === null || !(bpm > 0) || !(to > from)) {
    return out;
  }
  const spacing = 60 / bpm;
  if (spacing * pps < MIN_BEAT_SPACING_PX) {
    return out;
  }
  const first = Math.ceil((from - anchor) / spacing);
  const last = Math.floor((to - anchor) / spacing);
  for (let n = first; n <= last; n += 1) {
    const index = ((n % perBar) + perBar) % perBar;
    out.push({ time: anchor + n * spacing, downbeat: index === 0 });
  }
  return out;
}
