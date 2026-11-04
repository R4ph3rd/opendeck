/** Manual scale choices of Settings, Interface. `auto` follows the window height. */
export type UiScalePref = "auto" | 1 | 1.25 | 1.5 | 1.75 | 2;

export const UI_SCALE_CHOICES: readonly UiScalePref[] = ["auto", 1, 1.25, 1.5, 1.75, 2];

/** Window height that gets scale 1. */
export const BASE_HEIGHT = 1080;
const STEP = 0.125;

/** Auto scale: `clamp(1, height / 1080, 2)` rounded to 0.125 steps. */
export function autoScale(viewportHeight: number): number {
  const raw = viewportHeight / BASE_HEIGHT;
  const clamped = Math.min(2, Math.max(1, Number.isFinite(raw) ? raw : 1));
  return Math.round(clamped / STEP) * STEP;
}

export function resolveScale(pref: UiScalePref, viewportHeight: number): number {
  return pref === "auto" ? autoScale(viewportHeight) : pref;
}

/** Accepts a stored value; anything unknown becomes `auto`. */
export function parseScalePref(value: unknown): UiScalePref {
  return UI_SCALE_CHOICES.includes(value as UiScalePref) ? (value as UiScalePref) : "auto";
}

/** Sets `--ui-scale` on the root. Base CSS turns it into the root font size; rem tokens and utilities scale together. */
export function applyUiScale(scale: number): void {
  document.documentElement.style.setProperty("--ui-scale", String(scale));
}
