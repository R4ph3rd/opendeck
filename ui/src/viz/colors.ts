/**
 * Design token colours for canvas drawing. Canvas cannot use `var(--x)`, so the
 * resolved values are read from computed style at mount and again whenever the
 * theme changes. Nothing in `viz` writes a colour literal.
 */

/** Token names the viz components draw with, without the leading `--`. */
export const TOKEN_NAMES = [
  "bg",
  "bg-subtle",
  "surface",
  "surface-raised",
  "surface-hover",
  "border",
  "fg",
  "fg-muted",
  "fg-subtle",
  "accent",
  "danger",
  "success",
  "warning",
  "wave-low",
  "wave-mid",
  "wave-high",
  "playhead",
  "beat-grid",
  "beat-grid-downbeat",
  "loop-region",
  "loop-edge",
  "meter-green",
  "meter-amber",
  "meter-red",
  "meter-clip",
  "meter-off",
  "deck-a",
  "deck-b",
  "deck-c",
  "deck-d",
  "cue-1",
  "cue-2",
  "cue-3",
  "cue-4",
  "cue-5",
  "cue-6",
  "cue-7",
  "cue-8",
  "cue-fg",
] as const;

export type TokenName = (typeof TOKEN_NAMES)[number];

/** Resolved colour strings by token, plus the numeric opacity tokens. */
export interface VizColors {
  readonly color: Readonly<Record<TokenName, string>>;
  readonly playedOpacity: number;
  readonly unplayedOpacity: number;
  /** `font-family` of the canvas element, for labels drawn on it. */
  readonly font: string;
}

function number(style: CSSStyleDeclaration, name: string, fallback: number): number {
  const parsed = Number.parseFloat(style.getPropertyValue(name));
  return Number.isFinite(parsed) ? parsed : fallback;
}

/** Reads every token from the computed style of `element`. */
export function readColors(element: Element): VizColors {
  const style = getComputedStyle(element);
  const color = {} as Record<TokenName, string>;
  for (const name of TOKEN_NAMES) {
    color[name] = style.getPropertyValue(`--${name}`).trim();
  }
  return {
    color,
    playedOpacity: number(style, "--wave-played-opacity", 0.45),
    unplayedOpacity: number(style, "--wave-unplayed-opacity", 1),
    font: style.fontFamily || "sans-serif",
  };
}

/** Hot cue colour token for a pad slot `0..8`. */
export function cueToken(index: number): TokenName {
  const n = ((Math.max(0, Math.floor(index)) % 8) + 1) as 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
  return `cue-${n}`;
}

/** Deck accent token. */
export function deckToken(deck: number): TokenName {
  return (["deck-a", "deck-b", "deck-c", "deck-d"] as const)[deck] ?? "deck-a";
}

/**
 * Calls `listener` when the theme may have changed: a `data-theme` or `class`
 * change on the root or on any ancestor of `element`, or a colour scheme
 * change. Returns the unsubscribe function.
 */
export function onThemeChange(element: Element, listener: () => void): () => void {
  const observer = new MutationObserver(listener);
  const options = {
    attributes: true,
    attributeFilter: ["data-theme", "class"],
  };
  for (let node: Element | null = element; node; node = node.parentElement) {
    observer.observe(node, options);
  }
  const media = globalThis.matchMedia?.("(prefers-color-scheme: dark)");
  media?.addEventListener?.("change", listener);
  return () => {
    observer.disconnect();
    media?.removeEventListener?.("change", listener);
  };
}

/** `prefers-reduced-motion: reduce` right now. */
export function prefersReducedMotion(): boolean {
  return globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
}
