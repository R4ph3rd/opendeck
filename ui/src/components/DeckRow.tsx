import { DECK_A, DECK_B } from "../state/app";
import { DeckPanel } from "./DeckPanel";
import { MixerPanel } from "./MixerPanel";

/** Deck 1, the slim mixer column and deck 2. The mixer width is the `--mixer-w` token. */
export function DeckRow() {
  return (
    <div className="grid size-full min-h-0 grid-cols-[minmax(0,1fr)_var(--mixer-w)_minmax(0,1fr)] gap-px bg-border">
      <DeckPanel deck={DECK_A} />
      <MixerPanel />
      <DeckPanel deck={DECK_B} />
    </div>
  );
}
