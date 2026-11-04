/**
 * Layout constants the resizable bands need as numbers. Everything that is
 * only CSS (deck header height, mixer width, jog size) lives in the design
 * tokens instead: see `--toolbar-h`, `--deck-header-h`, `--mixer-w` and the
 * rest of the LAYOUT SIZES group in `tokens.css`.
 */

/** Height of the top bar, px (`--toolbar-h`). */
export const TOP_BAR_PX = 28;

/** Smallest height of each stacked band, px. The sum fits a 1280 x 720 window. */
export const BAND_MIN_PX = {
  waves: 72,
  decks: 220,
  middle: 96,
  library: 120,
} as const;

/** Smallest width of the deck row columns, px. `mixer` follows `--mixer-w`. */
export const DECK_COLUMN_MIN_PX = { deck: 400, mixer: 240 } as const;

/**
 * Size for a resizable panel from a px value at UI scale 1, as rem (`72` gives
 * `"4.5rem"`) so it follows the UI scale like the rem design tokens.
 */
export const px = (value: number): string => `${value / 16}rem`;
