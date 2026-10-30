import { describe, expect, it } from "vitest";
import {
  PLATTER_RPM,
  RING_START,
  SEARCH_BEATS_PER_REV,
  TurnAccumulator,
  angleDelta,
  angleOf,
  platterAngle,
  searchBeats,
  toRevolutions,
  zoneAt,
} from "./jogMath";

describe("angles", () => {
  it("measures clockwise on screen as positive", () => {
    expect(angleOf(1, 0)).toBeCloseTo(0);
    expect(angleOf(0, 1)).toBeCloseTo(Math.PI / 2); // y down: 6 o'clock
    expect(angleDelta(angleOf(1, 0), angleOf(0, 1))).toBeCloseTo(Math.PI / 2);
  });
  it("wraps across the seam instead of jumping a full turn", () => {
    const a = Math.PI - 0.1;
    const b = -Math.PI + 0.1;
    expect(angleDelta(a, b)).toBeCloseTo(0.2);
    expect(angleDelta(b, a)).toBeCloseTo(-0.2);
  });
  it("converts to revolutions", () => {
    expect(toRevolutions(Math.PI)).toBeCloseTo(0.5);
    expect(toRevolutions(-2 * Math.PI)).toBeCloseTo(-1);
  });
  it("a quarter turn drag sums to a quarter revolution over many moves", () => {
    let prev = angleOf(1, 0);
    let rev = 0;
    for (let i = 1; i <= 30; i += 1) {
      const a = (i / 30) * (Math.PI / 2);
      const cur = angleOf(Math.cos(a), Math.sin(a));
      rev += toRevolutions(angleDelta(prev, cur));
      prev = cur;
    }
    expect(rev).toBeCloseTo(0.25, 6);
  });
});

describe("zones", () => {
  it("splits platter, ring and outside", () => {
    expect(zoneAt(0, 0, 100)).toBe("top");
    expect(zoneAt(RING_START * 100 - 1, 0, 100)).toBe("top");
    expect(zoneAt(RING_START * 100 + 1, 0, 100)).toBe("ring");
    expect(zoneAt(0, -100, 100)).toBe("ring");
    expect(zoneAt(80, 80, 100)).toBeNull();
    expect(zoneAt(1, 1, 0)).toBeNull();
  });
});

describe("platter", () => {
  it("turns at 33 1/3 rpm", () => {
    // 1.8 s per revolution.
    const full = platterAngle(1.8);
    expect(Math.min(full, 2 * Math.PI - full)).toBeCloseTo(0, 6);
    expect(platterAngle(0.9)).toBeCloseTo(Math.PI, 6);
    expect(60 / PLATTER_RPM).toBeCloseTo(1.8);
  });
  it("stays within one turn", () => {
    for (const p of [0, 1, 123.456, 9999]) {
      const a = platterAngle(p);
      expect(a).toBeGreaterThanOrEqual(0);
      expect(a).toBeLessThan(2 * Math.PI);
    }
  });
});

describe("search and accumulation", () => {
  it("scales a shift drag to beats", () => {
    expect(searchBeats(0.5)).toBe(SEARCH_BEATS_PER_REV / 2);
    expect(searchBeats(-1)).toBe(-SEARCH_BEATS_PER_REV);
  });
  it("sums turns between flushes", () => {
    const acc = new TurnAccumulator();
    acc.add(0.01);
    acc.add(-0.004);
    acc.add(0.02);
    expect(acc.take()).toBeCloseTo(0.026);
    expect(acc.take()).toBe(0);
  });
});
