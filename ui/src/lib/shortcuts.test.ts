import { describe, expect, it } from "vitest";
import {
  SHORTCUTS,
  findConflicts,
  findShortcut,
  isBindable,
  keyLabel,
  resolveShortcuts,
  setActiveShortcuts,
  shortcutLabel,
  withBinding,
} from "./shortcuts";

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

describe("shortcut registry", () => {
  it("gives every shortcut a unique id", () => {
    expect(new Set(SHORTCUTS.map((s) => s.id)).size).toBe(SHORTCUTS.length);
    expect(SHORTCUTS.find((s) => s.id === "0:play")?.code).toBe("Space");
    expect(SHORTCUTS.find((s) => s.id === "1:hotCue:2")?.code).toBe("Numpad3");
  });

  it("applies overrides and ignores unknown ids and reserved keys", () => {
    const list = resolveShortcuts({ "0:play": "KeyG", "0:cue": "Escape", nope: "KeyZ" });
    expect(list.find((s) => s.id === "0:play")).toMatchObject({ code: "KeyG", label: "G" });
    expect(list.find((s) => s.id === "0:cue")?.code).toBe("F1");
    expect(list).toHaveLength(SHORTCUTS.length);
  });

  it("flags actions that share a key", () => {
    expect(findConflicts(SHORTCUTS).size).toBe(0);
    const conflicts = findConflicts(resolveShortcuts({ "0:play": "KeyS" }));
    expect(conflicts.get("0:play")).toEqual(["0:sync"]);
    expect(conflicts.get("0:sync")).toEqual(["0:play"]);
  });

  it("leaves unbound actions out of conflicts", () => {
    const list = resolveShortcuts({ "0:play": "", "1:play": "" });
    expect(findConflicts(list).size).toBe(0);
  });

  it("drops an override that equals the default", () => {
    const changed = withBinding({}, "0:play", "KeyG");
    expect(changed).toEqual({ "0:play": "KeyG" });
    expect(withBinding(changed, "0:play", "Space")).toEqual({});
  });

  it("uses the active bindings for lookups and tooltips", () => {
    setActiveShortcuts(resolveShortcuts({ "0:play": "KeyG", "0:sync": "" }));
    try {
      expect(findShortcut("KeyG")).toMatchObject({ deck: 0, action: { kind: "play" } });
      expect(findShortcut("Space")).toBeUndefined();
      expect(shortcutLabel(0, { kind: "play" })).toBe("G");
      expect(shortcutLabel(0, { kind: "sync" })).toBeUndefined();
    } finally {
      setActiveShortcuts(SHORTCUTS);
    }
  });

  it("names keys for display", () => {
    expect(keyLabel("KeyS")).toBe("S");
    expect(keyLabel("Digit4")).toBe("4");
    expect(keyLabel("Numpad7")).toBe("Num 7");
    expect(keyLabel("Comma")).toBe(",");
    expect(keyLabel("")).toBe("Unbound");
    expect(isBindable("Escape")).toBe(false);
    expect(isBindable("KeyA")).toBe(true);
  });
});
