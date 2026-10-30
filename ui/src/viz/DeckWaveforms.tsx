import { useMemo } from "react";
import { commands } from "../ipc/bindings";
import { DECK_LETTERS } from "../lib/deck";
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
export function DeckWaveform({ deck }: { deck: number }) {
  const trackId = useAppStore(selectDeckUi(deck)).track?.id ?? null;
  const peaks = useTrackPeaks(trackId);
  const markers = useDeckMarkers(deck);
  const source = useDeckSource(deck);
  const zoom = useVizStore((s) => s.zoom[deck] ?? 0);
  const setZoom = useVizStore((s) => s.setZoom);
  return (
    <Waveform
      label={`Deck ${letter(deck)} waveform`}
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
export function DeckOverview({ deck, label }: { deck: number; label?: string }) {
  const trackId = useAppStore(selectDeckUi(deck)).track?.id ?? null;
  const peaks = useTrackPeaks(trackId);
  const markers = useDeckMarkers(deck);
  const source = useDeckSource(deck);
  // Without an overview level, the coarsest detail level covers the whole track too.
  const level = peaks.overview ?? peaks.detail[2] ?? peaks.detail[1] ?? peaks.detail[0] ?? null;
  return (
    <Overview
      label={label ?? `Deck ${letter(deck)} overview`}
      level={level}
      markers={markers}
      frameSource={source}
      onSeek={(seconds) => seek(deck, seconds)}
    />
  );
}

/** The two zoomed waveforms stacked, deck A above deck B, as on a DJ controller. */
export function WaveformStack({ decks }: { decks: readonly number[] }) {
  return (
    <section aria-label="Waveforms" className="flex shrink-0 flex-col gap-1">
      {decks.map((deck) => (
        <DeckWaveform key={deck} deck={deck} />
      ))}
    </section>
  );
}
