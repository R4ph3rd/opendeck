import { describe, expect, it } from "vitest";
import { readColors, TOKEN_NAMES } from "./colors";
import { MockDeck, mockMarkers, synthPeaks } from "./mock";
import { levelSeconds } from "./peaks";
import { createWaveformScratch, drawWaveform } from "./waveformDraw";
import type { CanvasFrame } from "./useCanvasLoop";

/** Canvas context that accepts every call and draws nothing, so only our JS is timed. */
function stubContext(): CanvasRenderingContext2D {
  return new Proxy({} as CanvasRenderingContext2D, {
    get: (_t, key) => (key === "canvas" ? null : () => undefined),
    set: () => true,
  });
}

describe("synthetic track", () => {
  it("is deterministic and shaped like the analysis levels", () => {
    const a = synthPeaks(60);
    const b = synthPeaks(60);
    expect(a.detail[0]?.points).toEqual(b.detail[0]?.points);
    expect(a.detail.map((l) => l?.framesPerPoint)).toEqual([64, 256, 1024]);
    expect(a.overview?.count).toBeGreaterThan(1000);
    expect(a.overview?.count).toBeLessThanOrEqual(2200);
    const coarsest = a.detail[2];
    expect(coarsest ? levelSeconds(coarsest) : 0).toBeCloseTo(60, 0);
  });

  it("keeps a coarse level's maxima", () => {
    const t = synthPeaks(30);
    const fine = t.detail[0]?.points ?? new Uint8Array(0);
    const coarse = t.detail[1]?.points ?? new Uint8Array(0);
    let max = 0;
    for (let i = 0; i < 4; i += 1) max = Math.max(max, fine[i * 4] ?? 0);
    expect(coarse[0]).toBe(max);
  });
});

describe("waveform frame cost (JS only, canvas calls stubbed)", () => {
  it("draws a frame of a 10 minute track well inside a 60 fps budget", () => {
    const duration = 600;
    const peaks = synthPeaks(duration);
    const markers = mockMarkers(duration);
    const deck = new MockDeck(duration, { position: 120 });
    const width = 2400; // 1200 css px at dpr 2
    const frame = (now: number): CanvasFrame => ({
      ctx: stubContext(),
      width,
      height: 160,
      dpr: 2,
      colors: readColors(document.documentElement),
      now,
      invalidated: false,
    });
    expect(TOKEN_NAMES.length).toBeGreaterThan(30);
    const scratch = createWaveformScratch();
    const colors = readColors(document.documentElement);
    const run = (zoomIndex: number, frames: number): number => {
      const times: number[] = [];
      for (let i = 0; i < frames; i += 1) {
        const now = 1000 + i * 16.7;
        const f = { ...frame(now), colors };
        const state = {
          levels: peaks.detail,
          deck: { ...deck.source(now), position: 120 + i * 0.0167 },
          markers,
          zoomIndex,
          scrub: null,
        };
        const t0 = performance.now();
        drawWaveform(f, state, scratch);
        times.push(performance.now() - t0);
      }
      times.sort((x, y) => x - y);
      return times[Math.floor(times.length * 0.95)] ?? 0;
    };
    run(2, 30); // warm up
    const p95 = [0, 2, 5].map((z) => run(z, 120));
    console.info(`waveform p95 frame cost ms (zoom 2s, 8s, 64s): ${p95.map((v) => v.toFixed(2)).join(", ")}`);
    // Loose bound: a 60 fps frame is 16.7 ms and two decks draw per frame.
    for (const v of p95) expect(v).toBeLessThan(6);
  });
});
