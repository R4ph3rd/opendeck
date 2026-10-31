import { describe, expect, it } from "vitest";
import { formatClock } from "./format";
import { keyPitchClass, keySyncShift } from "./keySync";

describe("keySync", () => {
  it("reads keys as their relative major", () => {
    expect(keyPitchClass("C")).toBe(0);
    expect(keyPitchClass("Am")).toBe(0);
    expect(keyPitchClass("F#m")).toBe(9);
    expect(keyPitchClass("Bb")).toBe(10);
    expect(keyPitchClass("nope")).toBeNull();
    expect(keyPitchClass(null)).toBeNull();
  });

  it("finds the shortest shift onto the other deck's key", () => {
    expect(keySyncShift("Am", "Am", 0)).toBe(0);
    expect(keySyncShift("Am", "Cm", 0)).toBe(3);
    expect(keySyncShift("Cm", "Am", 0)).toBe(-3);
    expect(keySyncShift("C", "G", 0)).toBe(-5);
    // The other deck is already shifted: follow its sounding key.
    expect(keySyncShift("C", "C", 2)).toBe(2);
    expect(keySyncShift("C", null, 0)).toBeNull();
  });
});

describe("formatClock", () => {
  it("shows minutes, seconds and tenths", () => {
    expect(formatClock(0)).toBe("00:00.0");
    expect(formatClock(113.67)).toBe("01:53.6");
    expect(formatClock(-4)).toBe("00:00.0");
  });
});
