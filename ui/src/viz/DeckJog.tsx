import { Toggle, cn } from "@opendeck/design-system";
import { useCallback, useMemo, useState } from "react";
import { commands } from "../ipc/bindings";
import { DECK_LETTERS } from "../lib/deck";
import { formatTempo } from "../lib/format";
import { selectDeckUi, TEMPO_RANGE_FRACTION, useAppStore } from "../state/app";
import { telemetryStore } from "../state/telemetry";
import { telemetryDeckFrameSource } from "./deckFrame";
import { JogWheel, type JogReadout } from "./JogWheel";

/** Jog commands fire at frame rate; failures are dropped instead of toasting each one. */
const quiet = (pending: Promise<unknown>): void => {
  pending.catch(() => undefined);
};

/** Percent of range shown after the plus-minus sign, `6` for the 6% range. */
const rangePercent = (fraction: number): string => String(Math.round(fraction * 100));

/**
 * Jog wheel of one deck wired to `jog_touch`, `jog_turn` and `jog_search`.
 * The centre shows the effective BPM and the tempo change with its range.
 */
export function DeckJog({ deck, disabled = false }: { deck: number; disabled?: boolean }) {
  const letter = DECK_LETTERS[deck] ?? String(deck + 1);
  const source = useMemo(() => telemetryDeckFrameSource(deck), [deck]);
  const { cuePoint, tempoFader, tempoRange } = useAppStore(selectDeckUi(deck));
  const fraction = TEMPO_RANGE_FRACTION[tempoRange];
  const sub = `${formatTempo(tempoFader, fraction)} ±${rangePercent(fraction)}`;
  const readout = useCallback((): JogReadout => {
    const bpm = telemetryStore.getState().telemetry?.decks[deck]?.bpm ?? null;
    return { main: bpm !== null && bpm > 0 ? bpm.toFixed(2) : "--", sub };
  }, [deck, sub]);
  return (
    <JogWheel
      deck={deck}
      label={`Deck ${letter} jog wheel`}
      frameSource={source}
      cuePoint={cuePoint}
      readout={readout}
      disabled={disabled}
      onTouch={(touched) => quiet(commands.jogTouch(deck, touched))}
      onTurn={(revolutions) => quiet(commands.jogTurn(deck, revolutions))}
      onSearch={(beats) => quiet(commands.jogSearch(deck, beats))}
    />
  );
}

/** Vinyl mode toggle of a deck (engine default is on, `DeckState.vinyl_mode`). */
export function VinylToggle({ deck, className }: { deck: number; className?: string }) {
  const letter = DECK_LETTERS[deck] ?? String(deck + 1);
  const [vinyl, setVinyl] = useState(true);
  return (
    <Toggle
      size="sm"
      aria-label={`Vinyl mode ${letter}`}
      pressed={vinyl}
      className={cn(
        "rounded-sm bg-surface-raised px-1.5 ds-label data-[state=on]:text-accent data-[state=on]:shadow-[inset_0_0_0_1px_var(--accent)]",
        className,
      )}
      onPressedChange={(on) => {
        setVinyl(on);
        quiet(commands.setVinylMode(deck, on));
      }}
    >
      VINYL
    </Toggle>
  );
}
