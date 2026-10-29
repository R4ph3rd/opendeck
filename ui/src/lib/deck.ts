/** Channel letters, indexed by deck. */
export const DECK_LETTERS = ["A", "B", "C", "D"] as const;

export function deckLetter(deck: number): string {
  return DECK_LETTERS[deck] ?? String(deck + 1);
}

/**
 * Deck accent classes. Tailwind needs whole class names in the source, so the
 * token colours are listed per deck instead of built from a template.
 */
export const DECK_BORDER = [
  "border-t-deck-a",
  "border-t-deck-b",
  "border-t-deck-c",
  "border-t-deck-d",
] as const;

export const DECK_FILL = ["bg-deck-a", "bg-deck-b", "bg-deck-c", "bg-deck-d"] as const;

export const DECK_TEXT = ["text-deck-a", "text-deck-b", "text-deck-c", "text-deck-d"] as const;

/** Hot cue colours by slot (cue palette tokens). */
export const CUE_FILL = [
  "bg-cue-1",
  "bg-cue-2",
  "bg-cue-3",
  "bg-cue-4",
  "bg-cue-5",
  "bg-cue-6",
  "bg-cue-7",
  "bg-cue-8",
] as const;

export const CUE_TEXT = [
  "text-cue-1",
  "text-cue-2",
  "text-cue-3",
  "text-cue-4",
  "text-cue-5",
  "text-cue-6",
  "text-cue-7",
  "text-cue-8",
] as const;
