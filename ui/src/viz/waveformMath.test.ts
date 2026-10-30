import { describe, expect, it } from "vitest";
import {
  ZOOM_SECONDS,
  beatLines,
  clampZoom,
  pixelsPerSecond,
  scrubDelta,
  stepZoom,
  timeToX,
  xToTime,
} from "./waveformMath";

describe("zoom", () => {
  it("steps and clamps", () => {
    expect(stepZoom(2, 1)).toBe(1);
    expect(stepZoom(2, -1)).toBe(3);
    expect(stepZoom(0, 1)).toBe(0);
    expect(stepZoom(ZOOM_SECONDS.length - 1, -1)).toBe(ZOOM_SECONDS.length - 1);
    expect(clampZoom(99)).toBe(ZOOM_SECONDS.length - 1);
    expect(clampZoom(-4)).toBe(0);
  });
});

describe("time and x", () => {
  const width = 1000;
  const pps = pixelsPerSecond(width, 8);
  it("puts the playhead at the centre", () => {
    expect(timeToX(42.3, 42.3, width, pps)).toBe(500);
  });
  it("is linear and invertible", () => {
    expect(timeToX(44.3, 42.3, width, pps)).toBe(500 + 250);
    expect(xToTime(750, 42.3, width, pps)).toBeCloseTo(44.3, 6);
  });
  it("moves by whole pixels while scrolling (no shimmer)", () => {
    const a = timeToX(10, 20.0, width, pps);
    const b = timeToX(10, 20.001, width, pps);
    expect(Number.isInteger(a) && Number.isInteger(b)).toBe(true);
    expect(Math.abs(a - b)).toBeLessThanOrEqual(1);
  });
});

describe("scrubDelta", () => {
  it("drags the audio with the pointer", () => {
    // Half the width to the right pulls half a window of earlier audio in.
    expect(scrubDelta(250, 500, 8)).toBeCloseTo(-4, 9);
    expect(scrubDelta(-125, 500, 8)).toBeCloseTo(2, 9);
    expect(scrubDelta(10, 0, 8)).toBe(0);
  });
});

describe("beatLines", () => {
  const grid = { anchor: 1, bpm: 120, beats_per_bar: 4 };
  it("lists beats with downbeats every bar", () => {
    const lines = beatLines(grid, 1, 5, 100);
    expect(lines.map((l) => l.time)).toEqual([1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5]);
    expect(lines.filter((l) => l.downbeat).map((l) => l.time)).toEqual([1, 3, 5]);
  });
  it("extends before the anchor with correct bar positions", () => {
    const lines = beatLines(grid, -1.4, 0.9, 100);
    expect(lines.map((l) => l.time)).toEqual([-1, -0.5, 0, 0.5]);
    // Beat -2 is the downbeat of the previous bar: time -1.
    expect(lines[0]?.downbeat).toBe(true);
    expect(lines[1]?.downbeat).toBe(false);
  });
  it("skips lines that would be denser than a few pixels", () => {
    expect(beatLines(grid, 0, 60, 2)).toEqual([]);
  });
  it("ignores invalid grids", () => {
    expect(beatLines({ anchor: 0, bpm: null, beats_per_bar: 4 }, 0, 10, 100)).toEqual([]);
  });
});
