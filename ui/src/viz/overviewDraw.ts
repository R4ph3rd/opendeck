import type { DeckFrame } from "./deckFrame";
import { cueMarks, drawCueFlag, type DeckMarkers } from "./markers";
import { levelSeconds, sampleRange, type PeakLevelData, type PeakSample } from "./peaks";
import type { VizColors } from "./colors";
import type { CanvasFrame } from "./useCanvasLoop";

export interface OverviewState {
  /** Whole-track peaks: the overview level, or the coarsest detail level. */
  level: PeakLevelData | null;
  deck: DeckFrame;
  markers: DeckMarkers;
  /** Position override while the user drags, seconds. */
  scrub: number | null;
}

export interface OverviewScratch {
  cache: HTMLCanvasElement | null;
  cacheKey: { level: unknown; colors: unknown; w: number; h: number } | null;
  sample: PeakSample;
  key: string;
  markers: unknown;
}

export function createOverviewScratch(): OverviewScratch {
  return {
    cache: null,
    cacheKey: null,
    sample: { peak: 0, low: 0, mid: 0, high: 0 },
    key: "",
    markers: null,
  };
}

/** Renders the static whole-track waveform into `target`. */
function renderStatic(
  target: HTMLCanvasElement,
  level: PeakLevelData,
  colors: VizColors,
  w: number,
  h: number,
  dpr: number,
  sample: PeakSample,
): void {
  target.width = w;
  target.height = h;
  const ctx = target.getContext("2d");
  if (!ctx) {
    return;
  }
  ctx.clearRect(0, 0, w, h);
  const seconds = levelSeconds(level);
  const mid = h / 2;
  const half = mid * 0.94;
  const bars = new Uint8Array(w * 3);
  for (let x = 0; x < w; x += 1) {
    const a = ((x / w) * seconds * level.sampleRate) | 0;
    const b = (((x + 1) / w) * seconds * level.sampleRate) | 0;
    sampleRange(level, a, Math.max(b, a + 1), sample);
    bars[x * 3] = sample.low;
    bars[x * 3 + 1] = sample.mid;
    bars[x * 3 + 2] = sample.high;
  }
  const bands = ["wave-low", "wave-mid", "wave-high"] as const;
  bands.forEach((token, i) => {
    ctx.fillStyle = colors.color[token];
    ctx.beginPath();
    for (let x = 0; x < w; x += 1) {
      const v = bars[x * 3 + i] ?? 0;
      if (v > 0) {
        const hh = Math.max(dpr * 0.5, (v / 255) * half);
        ctx.rect(x, mid - hh, 1, hh * 2);
      }
    }
    ctx.fill();
  });
}

/**
 * Draws the full-track overview: cached waveform, elapsed region dimmed toward
 * the surface colour, loop and cue marks, playhead. The waveform is rendered
 * once per size, peak set and theme, so a frame costs a blit and a few rects.
 * Returns false when nothing changed.
 */
export function drawOverview(f: CanvasFrame, s: OverviewState, scratch: OverviewScratch): boolean {
  const { ctx, width: w, height: h, dpr, colors } = f;
  const duration = s.deck.duration > 0 ? s.deck.duration : s.level ? levelSeconds(s.level) : 0;
  const position = s.scrub ?? s.deck.position;
  const loop = s.deck.loop;
  const key = `${position.toFixed(3)}|${w}|${h}|${duration.toFixed(3)}|${loop?.start}|${loop?.end}|${s.deck.loopActive}`;
  const ck = scratch.cacheKey;
  const cacheValid =
    ck !== null && ck.level === s.level && ck.colors === colors && ck.w === w && ck.h === h;
  if (!f.invalidated && cacheValid && key === scratch.key && s.markers === scratch.markers) {
    return false;
  }
  scratch.key = key;
  scratch.markers = s.markers;

  ctx.clearRect(0, 0, w, h);
  if (s.level && duration > 0) {
    if (!cacheValid) {
      scratch.cache ??= document.createElement("canvas");
      renderStatic(scratch.cache, s.level, colors, w, h, dpr, scratch.sample);
      scratch.cacheKey = { level: s.level, colors, w, h };
    }
    if (scratch.cache) {
      ctx.drawImage(scratch.cache, 0, 0);
    }
  }
  if (duration <= 0) {
    return true;
  }
  const x = (t: number) => Math.round((Math.min(duration, Math.max(0, t)) / duration) * w);
  const px = x(position);

  // Elapsed region: fade the waveform toward the surface.
  ctx.globalAlpha = 1 - colors.playedOpacity;
  ctx.fillStyle = colors.color["surface"];
  ctx.fillRect(0, 0, px, h);
  ctx.globalAlpha = 1;

  if (loop) {
    const xa = x(loop.start);
    const xb = x(loop.end);
    ctx.globalAlpha = s.deck.loopActive ? 1 : 0.5;
    ctx.fillStyle = colors.color["loop-region"];
    ctx.fillRect(xa, 0, Math.max(1, xb - xa), h);
    ctx.fillStyle = colors.color["loop-edge"];
    ctx.fillRect(xa, 0, Math.max(1, Math.round(dpr)), h);
    ctx.fillRect(Math.max(xa, xb - Math.round(dpr)), 0, Math.max(1, Math.round(dpr)), h);
    ctx.globalAlpha = 1;
  }

  const lineW = Math.max(1, Math.round(dpr));
  for (const mark of cueMarks(s.markers)) {
    const mx = x(mark.time);
    ctx.fillStyle = colors.color[mark.token];
    ctx.fillRect(mx, 0, lineW, h);
    if (mark.hot) {
      drawCueFlag(ctx, colors, mark, Math.min(mx, w - 11 * dpr), 0, dpr);
    }
  }

  ctx.fillStyle = colors.color["playhead"];
  const headW = Math.max(2, Math.round(dpr * 2));
  ctx.fillRect(Math.min(px, w - headW), 0, headW, h);
  return true;
}
