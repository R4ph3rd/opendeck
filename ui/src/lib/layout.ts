/** Height of the top bar, px (`h-9`). */
export const TOP_BAR_PX = 36;

/** Smallest height of each stacked band, px. The sum fits a 1280 x 720 window. */
export const BAND_MIN_PX = {
  waves: 72,
  decks: 300,
  fx: 88,
  pads: 96,
  library: 120,
} as const;

/** Smallest width of the deck row columns, px. */
export const DECK_COLUMN_MIN_PX = { deck: 360, mixer: 288 } as const;

/** `"72px"` style size for a resizable panel. */
export const px = (value: number): string => `${value}px`;
