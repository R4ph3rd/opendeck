/**
 * Default keyboard shortcuts for both decks (docs/shortcuts.md). Keys are
 * `KeyboardEvent.code` values, so they stay on the same physical key on
 * every layout. The editable map arrives in M8.
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
      out.push({ code, deck, action: { kind }, label });
    }
    for (let index = 0; index < HOT_CUES; index++) {
      const n = index + 1;
      out.push(
        deck === 0
          ? { code: `Digit${n}`, deck, action: { kind: "hotCue", index }, label: String(n) }
          : { code: `Numpad${n}`, deck, action: { kind: "hotCue", index }, label: `Num ${n}` },
      );
    }
  }
  return out;
}

export const SHORTCUTS: readonly Shortcut[] = build();

const byCode = new Map(SHORTCUTS.map((s) => [s.code, s]));

/** Shortcut bound to a physical key, if any. */
export function findShortcut(code: string): Shortcut | undefined {
  return byCode.get(code);
}

/** Label of the key bound to `action` on `deck`, for tooltips. */
export function shortcutLabel(deck: number, action: ShortcutAction): string | undefined {
  return SHORTCUTS.find(
    (s) =>
      s.deck === deck &&
      s.action.kind === action.kind &&
      (s.action.kind !== "hotCue" ||
        action.kind !== "hotCue" ||
        s.action.index === action.index),
  )?.label;
}
