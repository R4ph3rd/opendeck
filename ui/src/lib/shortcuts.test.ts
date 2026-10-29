import { describe, expect, it } from "vitest";
import { SHORTCUTS, findShortcut, shortcutLabel } from "./shortcuts";

describe("shortcuts", () => {
  it("binds each physical key once", () => {
    const codes = SHORTCUTS.map((s) => s.code);
    expect(new Set(codes).size).toBe(codes.length);
  });

  it("gives both decks the same set of actions", () => {
    const kinds = (deck: number) =>
      SHORTCUTS.filter((s) => s.deck === deck)
        .map((s) => (s.action.kind === "hotCue" ? `hotCue${s.action.index}` : s.action.kind))
        .sort();
    expect(kinds(0)).toEqual(kinds(1));
    expect(kinds(0)).toHaveLength(10 + 8);
  });

  it("keeps the milestone 1 keys for deck A", () => {
    expect(findShortcut("Space")).toMatchObject({ deck: 0, action: { kind: "play" } });
    expect(findShortcut("F1")).toMatchObject({ deck: 0, action: { kind: "cue" } });
  });

  it("maps digits to deck A hot cues and the numpad to deck B", () => {
    expect(findShortcut("Digit1")).toMatchObject({ deck: 0, action: { kind: "hotCue", index: 0 } });
    expect(findShortcut("Digit8")).toMatchObject({ deck: 0, action: { kind: "hotCue", index: 7 } });
    expect(findShortcut("Numpad5")).toMatchObject({ deck: 1, action: { kind: "hotCue", index: 4 } });
    expect(findShortcut("Digit9")).toBeUndefined();
  });

  it("labels keys for tooltips", () => {
    expect(shortcutLabel(1, { kind: "sync" })).toBe("K");
    expect(shortcutLabel(0, { kind: "hotCue", index: 2 })).toBe("3");
    expect(shortcutLabel(1, { kind: "hotCue", index: 2 })).toBe("Num 3");
  });
});
