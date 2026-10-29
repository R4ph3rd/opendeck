import { describe, expect, it } from "vitest";
import {
  EQ_DEFAULT,
  EQ_MAX_GAIN_DB,
  LEVEL_DEFAULT,
  TRIM_DEFAULT,
  TRIM_MAX_GAIN_DB,
  dbToGain,
  eqGain,
  eqPosition,
  formatEq,
  formatFilter,
  formatLevel,
  formatTrim,
  gainToDb,
  levelGain,
  levelPosition,
  trimGain,
  trimPosition,
} from "./mixer";

describe("mixer curves", () => {
  it("converts between dB and gain", () => {
    expect(dbToGain(0)).toBe(1);
    expect(dbToGain(-20)).toBeCloseTo(0.1, 6);
    expect(gainToDb(1)).toBe(0);
    expect(gainToDb(0)).toBe(Number.NEGATIVE_INFINITY);
  });

  it("trim is unity in the middle and spans plus or minus the contract range", () => {
    expect(trimGain(TRIM_DEFAULT)).toBe(1);
    expect(gainToDb(trimGain(1))).toBeCloseTo(TRIM_MAX_GAIN_DB, 6);
    expect(gainToDb(trimGain(0))).toBeCloseTo(-TRIM_MAX_GAIN_DB, 6);
    expect(trimGain(5)).toBeCloseTo(trimGain(1), 9);
  });

  it("EQ kills at zero, is unity in the middle and boosts to the contract maximum", () => {
    expect(eqGain(0)).toBe(0);
    expect(eqGain(EQ_DEFAULT)).toBe(1);
    expect(gainToDb(eqGain(1))).toBeCloseTo(EQ_MAX_GAIN_DB, 6);
    expect(eqGain(0.01)).toBeGreaterThan(0);
    expect(eqGain(0.25)).toBeLessThan(eqGain(0.4));
  });

  it("level is silent at zero, unity at the default and boosts above it", () => {
    expect(levelGain(0)).toBe(0);
    expect(levelGain(LEVEL_DEFAULT)).toBeCloseTo(1, 9);
    expect(gainToDb(levelGain(1))).toBeCloseTo(6, 6);
  });

  it("inverts every curve", () => {
    for (const p of [0, 0.1, 0.3, 0.5, 0.62, 0.9, 1]) {
      expect(trimPosition(trimGain(p))).toBeCloseTo(p, 6);
      expect(eqPosition(eqGain(p))).toBeCloseTo(p, 6);
      expect(levelPosition(levelGain(p))).toBeCloseTo(p, 6);
    }
    expect(eqPosition(0)).toBe(0);
    expect(levelPosition(0)).toBe(0);
  });

  it("formats readouts", () => {
    expect(formatTrim(0.5)).toBe("0.0 dB");
    expect(formatTrim(1)).toBe("+12.0 dB");
    expect(formatEq(0)).toBe("kill");
    expect(formatEq(1)).toBe("+6.0 dB");
    expect(formatLevel(0)).toBe("off");
    expect(formatLevel(LEVEL_DEFAULT)).toBe("0.0 dB");
    expect(formatFilter(0)).toBe("off");
    expect(formatFilter(-0.5)).toBe("LP 50%");
    expect(formatFilter(1)).toBe("HP 100%");
  });
});
