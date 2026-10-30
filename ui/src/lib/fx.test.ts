import { describe, expect, it } from "vitest";
import type { FxStateDto, ParamDescriptor } from "../ipc/bindings";
import { fxEffects, neutralFxState } from "../test/bindingsMock";
import { patchLocal } from "../state/fx";
import {
  assignedDecks,
  defaultNormalized,
  formatCfx,
  formatDivision,
  formatParam,
  nearestDivision,
  padIndex,
  toPlain,
} from "./fx";

const desc = (
  unit: ParamDescriptor["unit"],
  min: number,
  max: number,
  def: number,
  curve: ParamDescriptor["curve"] = { kind: "linear" },
): ParamDescriptor => ({ id: 0, name: "P", unit, min, max, default: def, curve });

describe("fx formatting", () => {
  it("writes beat divisions as fractions or whole beats", () => {
    expect(formatDivision(0.25)).toBe("1/4");
    expect(formatDivision(0.0625)).toBe("1/16");
    expect(formatDivision(0.75)).toBe("3/4");
    expect(formatDivision(1)).toBe("1");
    expect(formatDivision(16)).toBe("16");
    expect(formatDivision(null)).toBe("-");
  });

  it("converts normalized values like the contracts descriptor", () => {
    expect(toPlain(desc("none", 0, 2, 1), 0.5)).toBe(1);
    expect(toPlain(desc("hertz", 20, 20_000, 1000, { kind: "logarithmic" }), 0.5)).toBeCloseTo(
      632.455,
      2,
    );
    expect(toPlain(desc("none", 0, 2, 1, { kind: "stepped", steps: 3 }), 0.4)).toBe(1);
    expect(defaultNormalized(desc("percent", -100, 100, 0))).toBe(0.5);
    expect(
      defaultNormalized(desc("hertz", 20, 20_000, 632.4555, { kind: "logarithmic" })),
    ).toBeCloseTo(0.5, 3);
  });

  it("shows parameter values with their unit", () => {
    expect(formatParam(desc("percent", 0, 98, 55), 0.5)).toBe("49%");
    expect(formatParam(desc("hertz", 100, 12_000, 2000, { kind: "logarithmic" }), 1)).toBe(
      "12 kHz",
    );
    expect(formatParam(desc("hertz", 100, 12_000, 2000, { kind: "logarithmic" }), 0)).toBe("100 Hz");
    expect(formatParam(desc("milliseconds", 10, 4000, 500, { kind: "logarithmic" }), 1)).toBe(
      "4 s",
    );
    expect(formatParam(desc("decibels", -24, 12, 0), 0.5)).toBe("-6 dB");
    expect(formatParam(desc("semitones", -12, 12, 0), 1)).toBe("+12 st");
    expect(formatParam(desc("beats", 0.25, 4, 1), 0)).toBe("1/4");
  });

  it("reads the CFX knob as a side and a depth", () => {
    expect(formatCfx(0)).toBe("off");
    expect(formatCfx(-0.5)).toBe("L 50%");
    expect(formatCfx(1)).toBe("R 100%");
  });

  it("maps assignments to decks and pads to indices", () => {
    expect(assignedDecks("both")).toEqual([0, 1]);
    expect(assignedDecks("ch2")).toEqual([1]);
    expect(assignedDecks("none")).toEqual([]);
    expect(padIndex(0, false, 0)).toBe(0);
    expect(padIndex(0, true, 7)).toBe(15);
    expect(padIndex(1, false, 0)).toBe(16);
    expect(padIndex(1, true, 7)).toBe(31);
  });

  it("finds the nearest division by ratio", () => {
    expect(nearestDivision([0.25, 0.5, 1], 0.6)).toBe(1);
    expect(nearestDivision([0.25, 0.5, 1], 0.01)).toBe(0);
  });
});

describe("patchLocal", () => {
  const state = neutralFxState() as unknown as FxStateDto;
  const effects = fxEffects() as never;
  const slot = { unit: { target: "beat" as const, unit: 0 }, slot: 0 };

  it("applies knob ops without touching the input", () => {
    const next = patchLocal(state, { kind: "set_level", slot, value: 0.9 }, effects);
    expect(next.units[0]?.slots[0]?.params[1]).toBe(0.9);
    expect(state.units[0]?.slots[0]?.params[1]).toBe(0.5);
    expect(patchLocal(state, { kind: "set_wet", slot, wet: 0.2 }, effects).units[0]?.slots[0]?.wet).toBe(0.2);
    expect(
      patchLocal(state, { kind: "set_cfx", deck: 1, amount: -1 }, effects).cfx[1]?.amount,
    ).toBe(-1);
    expect(
      patchLocal(state, { kind: "set_send_level", deck: 0, bus: 1, level: 0.3 }, effects)
        .send_levels[0]?.[1],
    ).toBe(0.3);
  });

  it("leaves structural ops to the backend echo", () => {
    expect(patchLocal(state, { kind: "set_unit_on", unit: 0, on: true }, effects)).toBe(state);
    expect(
      patchLocal(state, { kind: "set_param", slot: { ...slot, slot: 9 }, param: 0, value: 1 }, effects),
    ).toBe(state);
  });
});
