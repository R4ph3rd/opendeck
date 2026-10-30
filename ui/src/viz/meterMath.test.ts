import { describe, expect, it } from "vitest";
import {
  AMBER_FROM,
  CLIP_HOLD_MS,
  HOLD_FALL_PER_S,
  HOLD_MS,
  RED_FROM,
  dbToFraction,
  newHold,
  updateClip,
  updateHold,
} from "./meterMath";

describe("scale", () => {
  it("places amber and red thresholds", () => {
    expect(AMBER_FROM).toBeCloseTo(0.8);
    expect(RED_FROM).toBeCloseTo(0.95);
    expect(dbToFraction(0)).toBe(1);
    expect(dbToFraction(-60)).toBe(0);
    expect(dbToFraction(-90)).toBe(0);
    expect(dbToFraction(6)).toBe(1);
  });
});

describe("peak hold", () => {
  it("rises at once and holds", () => {
    const h = newHold();
    expect(updateHold(h, 0.7, 100)).toBe(0.7);
    expect(updateHold(h, 0.2, 100 + HOLD_MS - 1)).toBe(0.7);
  });
  it("falls after the hold time and never below the live level", () => {
    const h = newHold();
    updateHold(h, 0.8, 0);
    updateHold(h, 0.1, HOLD_MS);
    const later = updateHold(h, 0.1, HOLD_MS + 500);
    expect(later).toBeCloseTo(0.8 - 0.5 * HOLD_FALL_PER_S, 5);
    expect(updateHold(h, 0.75, HOLD_MS + 600)).toBeGreaterThanOrEqual(0.75);
  });
  it("restarts the timer on a new peak", () => {
    const h = newHold();
    updateHold(h, 0.5, 0);
    updateHold(h, 0.6, 1000);
    expect(updateHold(h, 0.1, 1000 + HOLD_MS - 1)).toBe(0.6);
  });
});

describe("clip latch", () => {
  it("stays lit for a while after the last clip", () => {
    const latch = { t: 0 };
    expect(updateClip(latch, false, 10)).toBe(false);
    expect(updateClip(latch, true, 1000)).toBe(true);
    expect(updateClip(latch, false, 1000 + CLIP_HOLD_MS - 1)).toBe(true);
    expect(updateClip(latch, false, 1000 + CLIP_HOLD_MS + 1)).toBe(false);
  });
});
