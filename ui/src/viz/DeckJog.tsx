import { Toggle } from "@opendeck/design-system";
import { useMemo, useState } from "react";
import { commands } from "../ipc/bindings";
import { DECK_LETTERS } from "../lib/deck";
import { selectDeckUi, useAppStore } from "../state/app";
import { telemetryDeckFrameSource } from "./deckFrame";
import { JogWheel } from "./JogWheel";

/** Jog commands fire at frame rate; failures are dropped instead of toasting each one. */
const quiet = (pending: Promise<unknown>): void => {
  pending.catch(() => undefined);
};

/** Jog wheel of one deck wired to `jog_touch`, `jog_turn`, `jog_search` and the vinyl toggle. */
export function DeckJog({ deck, disabled = false }: { deck: number; disabled?: boolean }) {
  const letter = DECK_LETTERS[deck] ?? String(deck + 1);
  const source = useMemo(() => telemetryDeckFrameSource(deck), [deck]);
  const cue = useAppStore(selectDeckUi(deck)).cuePoint;
  // Engine default is vinyl mode on (`DeckState.vinyl_mode`).
  const [vinyl, setVinyl] = useState(true);
  return (
    <div className="flex flex-col items-center gap-1">
      <JogWheel
        deck={deck}
        label={`Deck ${letter} jog wheel`}
        frameSource={source}
        cuePoint={cue}
        disabled={disabled}
        onTouch={(touched) => quiet(commands.jogTouch(deck, touched))}
        onTurn={(revolutions) => quiet(commands.jogTurn(deck, revolutions))}
        onSearch={(beats) => quiet(commands.jogSearch(deck, beats))}
      />
      <Toggle
        variant="outline"
        size="sm"
        aria-label={`Vinyl mode ${letter}`}
        pressed={vinyl}
        onPressedChange={(on) => {
          setVinyl(on);
          quiet(commands.setVinylMode(deck, on));
        }}
      >
        VINYL
      </Toggle>
    </div>
  );
}
