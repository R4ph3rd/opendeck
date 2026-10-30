import { describe, expect, it } from "vitest";
import {
  MAX_EXTRAPOLATION_MS,
  nextAnchor,
  playheadAt,
  predicted,
  type PlayheadAnchor,
} from "./interp";

const anchor = (over: Partial<PlayheadAnchor> = {}): PlayheadAnchor => ({
  position: 10,
  rate: 1,
  playing: true,
  at: 1000,
  error: 0,
  ...over,
});

describe("playhead extrapolation", () => {
  it("advances by rate over wall time", () => {
    expect(playheadAt(anchor({ rate: 1.04 }), 1100)).toBeCloseTo(10.104, 6);
    expect(playheadAt(anchor({ rate: 0.5 }), 1200)).toBeCloseTo(10.1, 6);
  });

  it("stands still when paused", () => {
    expect(playheadAt(anchor({ playing: false }), 1200)).toBe(10);
  });

  it("stops extrapolating when telemetry stalls", () => {
    const stalled = playheadAt(anchor(), 1000 + 5000);
    expect(stalled).toBeCloseTo(10 + MAX_EXTRAPOLATION_MS / 1000, 6);
  });

  it("runs backwards at negative rates and never goes below zero", () => {
    expect(playheadAt(anchor({ position: 0.05, rate: -1 }), 1200)).toBe(0);
  });

  it("ignores a clock earlier than the anchor", () => {
    expect(predicted(anchor(), 900)).toBe(10);
  });
});

describe("nextAnchor", () => {
  it("starts exactly at the first snapshot", () => {
    const a = nextAnchor(null, 3, 1, true, 500);
    expect(playheadAt(a, 500)).toBe(3);
  });

  it("keeps the shown value continuous across a small correction", () => {
    const old = anchor();
    const shown = playheadAt(old, 1016);
    const next = nextAnchor(old, 10.03, 1, true, 1016);
    expect(playheadAt(next, 1016)).toBeCloseTo(shown, 9);
    // Bleeds towards the true value.
    const later = playheadAt(next, 1016 + 200);
    expect(later).toBeCloseTo(10.03 + 0.2, 2);
  });

  it("snaps on a seek", () => {
    const next = nextAnchor(anchor(), 60, 1, true, 1016);
    expect(playheadAt(next, 1016)).toBe(60);
  });

  it("snaps when the previous state was paused", () => {
    const next = nextAnchor(anchor({ playing: false }), 10.1, 1, true, 1016);
    expect(playheadAt(next, 1016)).toBe(10.1);
  });
});
