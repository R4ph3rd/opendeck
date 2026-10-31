import { cn } from "@opendeck/design-system";
import { useMemo } from "react";
import { commands } from "../ipc/bindings";
import { DECK_FILL, DECK_LETTERS } from "../lib/deck";
import { report } from "../state/actions";
import { selectDeckUi, useAppStore } from "../state/app";
import { telemetryDeckFrameSource } from "./deckFrame";
import { Overview } from "./Overview";
import { Waveform } from "./Waveform";
import { useDeckMarkers, useTrackPeaks, useVizStore } from "./useTrackData";

function useDeckSource(deck: number) {
  return useMemo(() => telemetryDeckFrameSource(deck), [deck]);
}

function seek(deck: number, seconds: number): void {
  void report(commands.seek(deck, seconds));
}

const letter = (deck: number) => DECK_LETTERS[deck] ?? String(deck + 1);

/** Zoomed waveform of one deck, wired to telemetry, peaks, cues and the seek command. */
export function DeckWaveform({ deck, className }: { deck: number; className?: string }) {
  const trackId = useAppStore(selectDeckUi(deck)).track?.id ?? null;
  const peaks = useTrackPeaks(trackId);
  const markers = useDeckMarkers(deck);
  const source = useDeckSource(deck);
  const zoom = useVizStore((s) => s.zoom[deck] ?? 0);
  const setZoom = useVizStore((s) => s.setZoom);
  return (
    <Waveform
      label={`Deck ${letter(deck)} waveform`}
      className={className}
      levels={peaks.detail}
      markers={markers}
      frameSource={source}
      zoomIndex={zoom}
      onZoomChange={(index) => setZoom(deck, index)}
      onScrub={(seconds) => seek(deck, seconds)}
    />
  );
}

/** Overview of one deck, click or drag to seek. */
export function DeckOverview({
  deck,
  label,
  className,
}: {
  deck: number;
  label?: string;
  className?: string;
}) {
  const trackId = useAppStore(selectDeckUi(deck)).track?.id ?? null;
  const peaks = useTrackPeaks(trackId);
  const markers = useDeckMarkers(deck);
  const source = useDeckSource(deck);
  // Without an overview level, the coarsest detail level covers the whole track too.
  const level = peaks.overview ?? peaks.detail[2] ?? peaks.detail[1] ?? peaks.detail[0] ?? null;
  return (
    <Overview
      label={label ?? `Deck ${letter(deck)} overview`}
      className={className}
      level={level}
      markers={markers}
      frameSource={source}
      onSeek={(seconds) => seek(deck, seconds)}
    />
  );
}

/**
 * The zoomed waveforms stacked, deck A above deck B, filling the band they
 * sit in. Each lane has a deck-coloured tag on the left edge.
 */
export function WaveformStack({ decks }: { decks: readonly number[] }) {
  return (
    <section aria-label="Waveforms" className="flex size-full min-h-0 flex-col gap-px bg-border">
      {decks.map((deck) => (
        <div key={deck} className="relative min-h-0 flex-1">
          <DeckWaveform deck={deck} className="h-full rounded-none" />
          <span
            aria-hidden
            className={cn(
              "pointer-events-none absolute left-0 top-0 grid size-5 place-items-center text-xs font-bold text-cue-fg",
              DECK_FILL[deck] ?? "",
            )}
          >
            {deck + 1}
          </span>
        </div>
      ))}
    </section>
  );
}
