/**
 * Default keyboard shortcuts for both decks (docs/shortcuts.md). Keys are
 * `KeyboardEvent.code` values, so they stay on the same physical key on
 * every layout. One registry: the defaults here, the user's overrides from
 * Settings, Keyboard, and the lookups the key handler and tooltips use.
 */

export type ShortcutAction =
  | { kind: "play" }
  | { kind: "cue" }
  | { kind: "sync" }
  | { kind: "quantize" }
  | { kind: "slip" }
  | { kind: "loopIn" }
  | { kind: "loopOut" }
  | { kind: "reloop" }
  | { kind: "jumpBack" }
  | { kind: "jumpForward" }
  | { kind: "hotCue"; index: number };

export interface Shortcut {
  /** Stable id (`0:play`, `1:hotCue:2`); the key of an override. */
  id: string;
  /** `KeyboardEvent.code`. */
  code: string;
  deck: number;
  action: ShortcutAction;
  /** Key names for the docs and tooltips. */
  label: string;
}

type PlainAction = Exclude<ShortcutAction["kind"], "hotCue">;

/** `[code, label]` per action for deck A, then deck B. */
const KEYS: Record<PlainAction, [[string, string], [string, string]]> = {
  play: [
    ["Space", "Space"],
    ["Enter", "Enter"],
  ],
  cue: [
    ["F1", "F1"],
    ["F2", "F2"],
  ],
  sync: [
    ["KeyS", "S"],
    ["KeyK", "K"],
  ],
  quantize: [
    ["KeyQ", "Q"],
    ["KeyP", "P"],
  ],
  slip: [
    ["KeyA", "A"],
    ["KeyL", "L"],
  ],
  loopIn: [
    ["KeyZ", "Z"],
    ["KeyN", "N"],
  ],
  loopOut: [
    ["KeyX", "X"],
    ["KeyM", "M"],
  ],
  reloop: [
    ["KeyC", "C"],
    ["Comma", ","],
  ],
  jumpBack: [
    ["KeyD", "D"],
    ["KeyH", "H"],
  ],
  jumpForward: [
    ["KeyF", "F"],
    ["KeyJ", "J"],
  ],
};

const HOT_CUES = 8;

function build(): Shortcut[] {
  const out: Shortcut[] = [];
  for (const deck of [0, 1]) {
    for (const kind of Object.keys(KEYS) as PlainAction[]) {
      const [code, label] = KEYS[kind][deck] ?? ["", ""];
      out.push({ id: `${deck}:${kind}`, code, deck, action: { kind }, label });
    }
    for (let index = 0; index < HOT_CUES; index++) {
      const n = index + 1;
      out.push(
        deck === 0
          ? {
              id: `${deck}:hotCue:${index}`,
              code: `Digit${n}`,
              deck,
              action: { kind: "hotCue", index },
              label: String(n),
            }
          : {
              id: `${deck}:hotCue:${index}`,
              code: `Numpad${n}`,
              deck,
              action: { kind: "hotCue", index },
              label: `Num ${n}`,
            },
      );
    }
  }
  return out;
}

/** The default bindings. */
export const SHORTCUTS: readonly Shortcut[] = build();

/** Overrides keyed by shortcut id, value a `KeyboardEvent.code`. */
export type ShortcutOverrides = Record<string, string>;

/** Display text of a physical key (`KeyS` is `S`, `Numpad1` is `Num 1`). */
export function keyLabel(code: string): string {
  if (code === "") {
    return "Unbound";
  }
  if (code.startsWith("Key")) {
    return code.slice(3);
  }
  if (code.startsWith("Digit")) {
    return code.slice(5);
  }
  if (code.startsWith("Numpad")) {
    const rest = code.slice(6);
    const named: Record<string, string> = {
      Add: "+",
      Subtract: "-",
      Multiply: "*",
      Divide: "/",
      Decimal: ".",
      Enter: "Enter",
    };
    return `Num ${named[rest] ?? rest}`;
  }
  const named: Record<string, string> = {
    Comma: ",",
    Period: ".",
    Slash: "/",
    Semicolon: ";",
    Quote: "'",
    Minus: "-",
    Equal: "=",
    BracketLeft: "[",
    BracketRight: "]",
    Backslash: "\\",
    Backquote: "`",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
  };
  return named[code] ?? code;
}

/** Keys that cannot be bound: they leave the dialog, move focus or are modifiers. */
const RESERVED = new Set([
  "Escape",
  "Tab",
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "ContextMenu",
  "CapsLock",
]);

/** Whether `code` may be bound to an action. */
export function isBindable(code: string): boolean {
  return code !== "" && !RESERVED.has(code);
}

/** The defaults with `overrides` applied (unknown ids are ignored). */
export function resolveShortcuts(overrides: ShortcutOverrides): Shortcut[] {
  return SHORTCUTS.map((s) => {
    const code = overrides[s.id];
    return typeof code === "string" && code !== s.code && (code === "" || isBindable(code))
      ? { ...s, code, label: keyLabel(code) }
      : s;
  });
}

/** Ids that share a key with another action, mapped to the ids they clash with. */
export function findConflicts(shortcuts: readonly Shortcut[]): Map<string, string[]> {
  const byCode = new Map<string, string[]>();
  for (const s of shortcuts) {
    if (s.code !== "") {
      byCode.set(s.code, [...(byCode.get(s.code) ?? []), s.id]);
    }
  }
  const out = new Map<string, string[]>();
  for (const ids of byCode.values()) {
    if (ids.length > 1) {
      for (const id of ids) {
        out.set(
          id,
          ids.filter((other) => other !== id),
        );
      }
    }
  }
  return out;
}

/** Overrides without the one for `id`. */
export function withoutBinding(overrides: ShortcutOverrides, id: string): ShortcutOverrides {
  return Object.fromEntries(Object.entries(overrides).filter(([key]) => key !== id));
}

/** Overrides after binding `id` to `code`; a code equal to the default drops the override. */
export function withBinding(overrides: ShortcutOverrides, id: string, code: string): ShortcutOverrides {
  const def = SHORTCUTS.find((s) => s.id === id);
  if (!def) {
    return { ...overrides };
  }
  const rest = withoutBinding(overrides, id);
  return code === def.code ? rest : { ...rest, [id]: code };
}

/** Human name of a shortcut for the editor (`Deck 1 Play`). */
export function shortcutName(s: Shortcut): string {
  const names: Record<PlainAction, string> = {
    play: "Play / pause",
    cue: "Cue",
    sync: "Sync",
    quantize: "Quantize",
    slip: "Slip",
    loopIn: "Loop in",
    loopOut: "Loop out",
    reloop: "Loop on / off",
    jumpBack: "Beat jump back",
    jumpForward: "Beat jump forward",
  };
  const what =
    s.action.kind === "hotCue" ? `Hot cue ${s.action.index + 1}` : names[s.action.kind];
  return `Deck ${s.deck + 1} ${what}`;
}

let active: readonly Shortcut[] = SHORTCUTS;
let byCode = new Map(active.map((s) => [s.code, s]));

/** Makes `shortcuts` the bindings `findShortcut` and `shortcutLabel` use. */
export function setActiveShortcuts(shortcuts: readonly Shortcut[]): void {
  active = shortcuts;
  byCode = new Map();
  for (const s of shortcuts) {
    if (s.code !== "" && !byCode.has(s.code)) {
      byCode.set(s.code, s);
    }
  }
}

/** Shortcut bound to a physical key, if any. */
export function findShortcut(code: string): Shortcut | undefined {
  return byCode.get(code);
}

/** Label of the key bound to `action` on `deck`, for tooltips. */
export function shortcutLabel(deck: number, action: ShortcutAction): string | undefined {
  const found = active.find(
    (s) =>
      s.deck === deck &&
      s.action.kind === action.kind &&
      (s.action.kind !== "hotCue" ||
        action.kind !== "hotCue" ||
        s.action.index === action.index),
  );
  return found && found.code !== "" ? found.label : undefined;
}
