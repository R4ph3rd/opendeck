import { cn } from "@opendeck/design-system";
import { DECK_FILL } from "../lib/deck";
import { DeckWaveform } from "../viz/DeckWaveforms";
import { useDeckDropZone } from "./deck/useDeckDrop";

/** One zoomed-waveform lane that takes tracks and audio files dropped on it. */
function Lane({ deck }: { deck: number }) {
  const { active, ref } = useDeckDropZone(deck);
  return (
    <div
      ref={ref}
      data-testid={`wave-lane-${deck}`}
      className={cn(
        "relative min-h-0 flex-1",
        active && "outline-2 -outline-offset-2 outline-accent",
      )}
    >
      <DeckWaveform deck={deck} className="h-full rounded-none" />
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute left-0 top-0 grid size-5 place-items-center text-xs font-semibold text-cue-fg",
          DECK_FILL[deck] ?? "",
        )}
      >
        {deck + 1}
      </span>
    </div>
  );
}

/** The zoomed waveforms stacked (row 1 is deck 1, row 2 deck 2); each row is a deck drop target. */
export function WaveformBand({ decks }: { decks: readonly number[] }) {
  return (
    <section aria-label="Waveforms" className="flex size-full min-h-0 flex-col gap-px bg-border">
      {decks.map((deck) => (
        <Lane key={deck} deck={deck} />
      ))}
    </section>
  );
}
