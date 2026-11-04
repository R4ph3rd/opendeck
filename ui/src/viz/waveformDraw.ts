import type { DeckFrame } from "./deckFrame";
import { cueMarks, drawCueFlag, type DeckMarkers } from "./markers";
import { aggregateColumns, selectLevel, type PeakLevelData, type PeakSample } from "./peaks";
import type { CanvasFrame } from "./useCanvasLoop";
import { ZOOM_SECONDS, beatLines, pixelsPerSecond, timeToX, type BeatLine } from "./waveformMath";

export interface WaveformState {
  /** Detail levels, any order, `null` while loading. */
  levels: readonly (PeakLevelData | null)[];
  deck: DeckFrame;
  markers: DeckMarkers;
  zoomIndex: number;
  /** Playhead override while the user scrubs, seconds. */
  scrub: number | null;
}

/** Reusable buffers and the last-drawn key, one per canvas. */
export interface WaveformScratch {
  bars: Uint8Array;
  sample: PeakSample;
  beats: BeatLine[];
  key: string;
  colors: unknown;
  levels: unknown;
  markers: unknown;
}

export function createWaveformScratch(): WaveformScratch {
  return {
    bars: new Uint8Array(0),
    sample: { peak: 0, low: 0, mid: 0, high: 0 },
    beats: [],
    key: "",
    colors: null,
    levels: null,
    markers: null,
  };
}

/**
 * Draws the zoomed, centred, scrolling waveform. Columns are one device pixel
 * wide and aligned to a fixed world grid (see `timeToX`), so data does not
 * shimmer as it scrolls. Bands are drawn low, mid, high on top of each other;
 * the part already played is dimmed with the `--wave-played-opacity` token.
 * Returns false when nothing changed and the canvas was left as is.
 */
export function drawWaveform(f: CanvasFrame, s: WaveformState, scratch: WaveformScratch): boolean {
  const { ctx, width: w, height: h, dpr, colors } = f;
  const position = s.scrub ?? s.deck.position;
  const visible = ZOOM_SECONDS[s.zoomIndex] ?? ZOOM_SECONDS[2];
  const loop = s.deck.loop;
  const key = `${position.toFixed(5)}|${w}|${h}|${s.zoomIndex}|${loop?.start}|${loop?.end}|${s.deck.loopActive}`;
  if (
    !f.invalidated &&
    key === scratch.key &&
    colors === scratch.colors &&
    s.levels === scratch.levels &&
    s.markers === scratch.markers
  ) {
    return false;
  }
  scratch.key = key;
  scratch.colors = colors;
  scratch.levels = s.levels;
  scratch.markers = s.markers;

  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, w, h);
  const pps = pixelsPerSecond(w, visible);
  const cx = Math.round(w / 2);
  const t0 = position - visible / 2;
  const t1 = position + visible / 2;
  const mid = h / 2;
  const midPx = Math.round(mid);

  // Loop region behind the waveform.
  if (loop) {
    const xa = timeToX(loop.start, position, w, pps);
    const xb = timeToX(loop.end, position, w, pps);
    ctx.globalAlpha = s.deck.loopActive ? 1 : 0.5;
    ctx.fillStyle = colors.color["loop-region"];
    ctx.fillRect(xa, 0, xb - xa, h);
    ctx.globalAlpha = 1;
  }

  // Band columns.
  const first = selectLevel(s.levels, 0);
  if (first) {
    const fpx = (first.sampleRate * visible) / w;
    const level = selectLevel(s.levels, fpx) ?? first;
    if (scratch.bars.length < w * 4) {
      scratch.bars = new Uint8Array(w * 4);
    }
    const bars = scratch.bars;
    const centreOffset = Math.round(position * pps);
    aggregateColumns(level, w, cx, centreOffset, pps, bars, scratch.sample);
    const half = Math.floor(mid * 0.94);
    const bands: readonly [number, "wave-low" | "wave-mid" | "wave-high"][] = [
      [1, "wave-low"],
      [2, "wave-mid"],
      [3, "wave-high"],
    ];
    const ranges: readonly [number, number, number][] = [
      [0, cx, colors.playedOpacity],
      [cx, w, colors.unplayedOpacity],
    ];
    for (const [from, to, alpha] of ranges) {
      ctx.globalAlpha = alpha;
      for (const [offset, token] of bands) {
        ctx.fillStyle = colors.color[token];
        // One hard-edged column per device pixel, whole-pixel heights.
        for (let x = from; x < to; x += 1) {
          const v = bars[x * 4 + offset] ?? 0;
          if (v > 0) {
            const hh = Math.max(1, Math.round((v / 255) * half));
            ctx.fillRect(x, midPx - hh, 1, hh * 2);
          }
        }
      }
    }
    ctx.globalAlpha = 1;
  }

  // Beat grid.
  if (s.markers.grid) {
    const lines = beatLines(s.markers.grid, t0, t1, pps, scratch.beats);
    const beatW = 1;
    for (const line of lines) {
      const x = timeToX(line.time, position, w, pps);
      ctx.fillStyle = line.downbeat ? colors.color["beat-grid-downbeat"] : colors.color["beat-grid"];
      ctx.fillRect(x, 0, beatW, h);
    }
  }

  // Loop edges on top of the grid.
  if (loop) {
    ctx.globalAlpha = s.deck.loopActive ? 1 : 0.6;
    ctx.fillStyle = colors.color["loop-edge"];
    const edge = Math.max(1, Math.round(dpr * 1.5));
    ctx.fillRect(timeToX(loop.start, position, w, pps), 0, edge, h);
    ctx.fillRect(timeToX(loop.end, position, w, pps) - edge, 0, edge, h);
    ctx.globalAlpha = 1;
  }

  // Cue marks, hot and memory.
  const lineW = 1;
  for (const mark of cueMarks(s.markers)) {
    if (mark.time < t0 - 1 || mark.time > t1 + 1) {
      continue;
    }
    const x = timeToX(mark.time, position, w, pps);
    ctx.fillStyle = colors.color[mark.token];
    ctx.fillRect(x, 0, lineW, h);
    drawCueFlag(ctx, colors, mark, x, 0, dpr);
  }

  // Playhead.
  const headW = 2;
  ctx.fillStyle = colors.color["playhead"];
  ctx.fillRect(cx - Math.floor(headW / 2), 0, headW, h);
  return true;
}
