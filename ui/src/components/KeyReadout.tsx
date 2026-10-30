import { Badge } from "@opendeck/design-system";
import { selectDeckUi, useAppStore } from "../state/app";
import { useLibraryStore } from "../state/library";

/** Analysed key of the track in a deck, in the notation chosen in the collection header. */
export function KeyReadout({ deck }: { deck: number }) {
  const ui = useAppStore(selectDeckUi(deck));
  const notation = useLibraryStore((s) => s.keyNotation);
  const track = ui.incoming ?? ui.track;
  const key = notation === "camelot" ? track?.key : track?.key_musical;
  return (
    <Badge variant={key ? "accent" : "neutral"} aria-label={`Deck ${deck + 1} key`} data-testid={`key-${deck}`}>
      {key ?? "--"}
    </Badge>
  );
}
