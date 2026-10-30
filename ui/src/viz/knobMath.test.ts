import { describe, expect, it } from "vitest";
import {
  CENTER_SNAP,
  DRAG_PIXELS,
  FINE_FACTOR,
  KNOB_START,
  KNOB_SWEEP,
  dragValue,
  keyStep,
  keyValue,
  snapCenter,
  snapToStep,
  unitToAngle,
  unitToValue,
  valueToUnit,
  wheelValue,
} from "./knobMath";

describe("value and angle", () => {
  it("maps the range to the 270 degree arc", () => {
    expect(unitToAngle(0)).toBeCloseTo(KNOB_START);
    expect(unitToAngle(1)).toBeCloseTo(KNOB_START + KNOB_SWEEP);
    expect(unitToAngle(0.5)).toBeCloseTo(KNOB_START + KNOB_SWEEP / 2);
  });
  it("round-trips and clamps", () => {
    expect(valueToUnit(0.5, 0, 2)).toBe(0.25);
    expect(unitToValue(valueToUnit(-0.3, -1, 1), -1, 1)).toBeCloseTo(-0.3);
    expect(valueToUnit(9, 0, 1)).toBe(1);
    expect(valueToUnit(1, 1, 1)).toBe(0);
  });
});

describe("dragging", () => {
  it("moves the full range over DRAG_PIXELS", () => {
    expect(dragValue(0, DRAG_PIXELS, 0, 1)).toBeCloseTo(1);
    expect(dragValue(1, -DRAG_PIXELS / 2, 0, 1)).toBeCloseTo(0.5);
  });
  it("is slower with shift", () => {
    expect(dragValue(0.5, 40, 0, 1, { fine: true }) - 0.5).toBeCloseTo((40 / DRAG_PIXELS) * FINE_FACTOR);
  });
  it("clamps to the range", () => {
    expect(dragValue(0.9, 1000, 0, 1)).toBe(1);
    expect(dragValue(0.1, -1000, 0, 1)).toBe(0);
  });
  it("snaps to a step", () => {
    expect(dragValue(0, 80.5, 0, 1, { step: 0.1 })).toBeCloseTo(0.5);
  });
});

describe("bipolar centre", () => {
  it("snaps a drag through the centre", () => {
    expect(dragValue(-0.2, 0.2 * (DRAG_PIXELS / 2) + 0.5, -1, 1, { bipolar: true })).toBe(0);
    expect(dragValue(-0.2, 0.2 * (DRAG_PIXELS / 2) + 0.5, -1, 1)).not.toBe(0);
  });
  it("snaps near the centre only", () => {
    expect(snapCenter(0.01, -1, 1, true)).toBe(0);
    expect(snapCenter(CENTER_SNAP * 2 + 0.01, -1, 1, true)).toBeCloseTo(CENTER_SNAP * 2 + 0.01);
    expect(snapCenter(0.01, -1, 1, false)).toBe(0.01);
  });
});

describe("snapToStep", () => {
  it("rounds to multiples from min", () => {
    expect(snapToStep(0.34, 0, 1, 0.1)).toBeCloseTo(0.3);
    expect(snapToStep(0.37, 0, 1, 0.1)).toBeCloseTo(0.4);
    expect(snapToStep(0.4, 0.05, 1, 0.1)).toBeCloseTo(0.45);
  });
  it("ignores a zero step", () => {
    expect(snapToStep(0.123, 0, 1, 0)).toBe(0.123);
  });
});

describe("keyboard", () => {
  it("steps by at least 1% of the range", () => {
    expect(keyStep(0, 1, 0.001)).toBeCloseTo(0.01);
    expect(keyStep(-1, 1, 0.5)).toBe(0.5);
  });
  it("handles arrows, page keys, home, end and reset", () => {
    expect(keyValue("ArrowUp", 0.5, 0, 1, 0.01, 0.7)).toBeCloseTo(0.51);
    expect(keyValue("ArrowDown", 0.5, 0, 1, 0.01, 0.7)).toBeCloseTo(0.49);
    expect(keyValue("PageUp", 0.5, 0, 1, 0.01, 0.7)).toBeCloseTo(0.6);
    expect(keyValue("Home", 0.5, 0, 1, 0.01, 0.7)).toBe(0);
    expect(keyValue("End", 0.5, 0, 1, 0.01, 0.7)).toBe(1);
    expect(keyValue("Enter", 0.5, 0, 1, 0.01, 0.7)).toBe(0.7);
    expect(keyValue("a", 0.5, 0, 1, 0.01, 0.7)).toBeNull();
    expect(keyValue("ArrowUp", 1, 0, 1, 0.01, 0.7)).toBe(1);
  });
});

describe("wheel", () => {
  it("scrolls up to increase", () => {
    expect(wheelValue(0.5, -100, 0, 1, 0.01, false)).toBeCloseTo(0.51);
    expect(wheelValue(0.5, 100, 0, 1, 0.01, false)).toBeCloseTo(0.49);
    expect(wheelValue(0.5, -100, 0, 1, 0.01, true)).toBeCloseTo(0.5 + 0.01 * FINE_FACTOR);
  });
});
