import { describe, expect, it } from "vitest";
import { autoScale, parseScalePref, resolveScale } from "./uiScale";

describe("uiScale", () => {
  it("is 1 at and below 1080p", () => {
    expect(autoScale(1080)).toBe(1);
    expect(autoScale(900)).toBe(1);
  });
  it("rounds to eighths and caps at 2", () => {
    expect(autoScale(1440)).toBe(1.375);
    expect(autoScale(2160)).toBe(2);
    expect(autoScale(4000)).toBe(2);
    expect(autoScale(1200)).toBe(1.125);
  });
  it("manual choices override auto", () => {
    expect(resolveScale(1.5, 2160)).toBe(1.5);
    expect(resolveScale("auto", 2160)).toBe(2);
  });
  it("falls back to auto for unknown stored values", () => {
    expect(parseScalePref(3)).toBe("auto");
    expect(parseScalePref("x")).toBe("auto");
    expect(parseScalePref(1.25)).toBe(1.25);
  });
});
