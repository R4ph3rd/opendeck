import { describe, expect, it } from "vitest";
import {
  EMPTY_SELECTION,
  clickSelect,
  contextTargets,
  focusAt,
  moveFocus,
  type IdAt,
} from "./selection";

/** Rows 0..99 with ids 100..199; rows 40 to 49 are not loaded. */
const idAt: IdAt = (i) => (i >= 40 && i < 50 ? null : i >= 0 && i < 100 ? i + 100 : null);
const none = { shift: false, ctrl: false };

describe("clickSelect", () => {
  it("selects only the clicked row", () => {
    const s = clickSelect(clickSelect(EMPTY_SELECTION, 3, none, idAt), 5, none, idAt);
    expect([...s.ids]).toEqual([105]);
    expect(s.anchor).toBe(5);
    expect(s.focus).toBe(5);
  });

  it("toggles rows with ctrl or cmd", () => {
    let s = clickSelect(EMPTY_SELECTION, 1, none, idAt);
    s = clickSelect(s, 3, { shift: false, ctrl: true }, idAt);
    expect([...s.ids].sort()).toEqual([101, 103]);
    s = clickSelect(s, 1, { shift: false, ctrl: true }, idAt);
    expect([...s.ids]).toEqual([103]);
  });

  it("selects a range from the anchor with shift", () => {
    let s = clickSelect(EMPTY_SELECTION, 2, none, idAt);
    s = clickSelect(s, 5, { shift: true, ctrl: false }, idAt);
    expect([...s.ids]).toEqual([102, 103, 104, 105]);
    expect(s.anchor).toBe(2);
    // Shrinking the range drops rows past the new end.
    s = clickSelect(s, 3, { shift: true, ctrl: false }, idAt);
    expect([...s.ids]).toEqual([102, 103]);
  });

  it("extends the selection with ctrl and shift, and skips rows that are not loaded", () => {
    let s = clickSelect(EMPTY_SELECTION, 90, none, idAt);
    s = clickSelect(s, 10, { shift: false, ctrl: true }, idAt);
    s = clickSelect(s, 52, { shift: true, ctrl: true }, idAt);
    expect(s.ids.has(190)).toBe(true);
    expect(s.ids.has(110)).toBe(true);
    expect(s.ids.has(152)).toBe(true);
    // Rows 40 to 49 are unloaded, so they are skipped, not selected as null.
    expect(s.ids.has(139)).toBe(true);
    expect(s.ids.has(140)).toBe(false);
    expect(s.ids.has(149)).toBe(false);
    expect(s.ids.has(150)).toBe(true);
  });

  it("ignores a click on a row that is not loaded", () => {
    const s = clickSelect(EMPTY_SELECTION, 45, none, idAt);
    expect(s).toBe(EMPTY_SELECTION);
  });
});

describe("keyboard focus", () => {
  it("moves and selects one row at a time, clamped to the ends", () => {
    let s = moveFocus(EMPTY_SELECTION, 1, 100, false, idAt);
    expect(s.focus).toBe(0);
    s = moveFocus(s, 1, 100, false, idAt);
    expect([...s.ids]).toEqual([101]);
    s = moveFocus(s, -5, 100, false, idAt);
    expect(s.focus).toBe(0);
    s = focusAt(s, 99, 100, false, idAt);
    s = moveFocus(s, 1, 100, false, idAt);
    expect(s.focus).toBe(99);
  });

  it("starts from the bottom when moving up with nothing focused", () => {
    expect(moveFocus(EMPTY_SELECTION, -1, 100, false, idAt).focus).toBe(99);
  });

  it("extends from the anchor with shift", () => {
    let s = focusAt(EMPTY_SELECTION, 10, 100, false, idAt);
    s = moveFocus(s, 1, 100, true, idAt);
    s = moveFocus(s, 1, 100, true, idAt);
    expect([...s.ids]).toEqual([110, 111, 112]);
    expect(s.anchor).toBe(10);
  });

  it("does nothing on an empty list", () => {
    expect(moveFocus(EMPTY_SELECTION, 1, 0, false, idAt)).toBe(EMPTY_SELECTION);
  });
});

describe("contextTargets", () => {
  it("acts on the whole selection when the row is in it, else on the row", () => {
    const s = { ids: new Set([1, 2, 3]), anchor: 0, focus: 0 };
    expect(contextTargets(s, 2).sort()).toEqual([1, 2, 3]);
    expect(contextTargets(s, 9)).toEqual([9]);
  });
});
