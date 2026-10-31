import { Panel, cn } from "@opendeck/design-system";
import { useState, type DragEvent } from "react";
import { useStore } from "zustand";
import { dragKind, readDrag } from "../lib/dnd";
import { DECK_BORDER } from "../lib/deck";
import { loadDroppedFile, loadTrackById } from "../state/libraryActions";
import { selectDeckUi, useAppStore } from "../state/app";
import { telemetryStore } from "../state/telemetry";
import { DeckOverview } from "../viz/DeckWaveforms";
import { DeckHeader } from "./deck/DeckHeader";
import { DeckMain } from "./deck/DeckMain";

interface DeckPanelProps {
  deck: number;
}

/**
 * One deck as an edge-to-edge band: header (title, clocks, key and sync),
 * overview waveform, then loop, transport, wheel, modes and tempo.
 */
export function DeckPanel({ deck }: DeckPanelProps) {
  const ui = useAppStore(selectDeckUi(deck));
  // Boolean slice: the component re-renders when this flips, not per tick.
  const loaded = useStore(telemetryStore, (s) => s.telemetry?.decks[deck]?.loaded ?? false);
  const ready = loaded && !ui.loading;
  const name = `Deck ${deck + 1}`;
  const [dropActive, setDropActive] = useState(false);

  // Tracks from the collection and audio files from the explorer load on drop.
  const onDragOver = (event: DragEvent) => {
    const kind = dragKind(event.dataTransfer);
    if (kind === "tracks" || kind === "files") {
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
      setDropActive(true);
    }
  };
  const onDrop = (event: DragEvent) => {
    setDropActive(false);
    const payload = readDrag(event.dataTransfer);
    if (payload?.kind === "tracks" && payload.ids[0] !== undefined) {
      event.preventDefault();
      void loadTrackById(deck, payload.ids[0]);
    } else if (payload?.kind === "files") {
      const file = payload.files.find((f) => f.kind === "audio");
      if (file) {
        event.preventDefault();
        void loadDroppedFile(deck, file);
      }
    }
  };

  return (
    <Panel
      flush
      aria-label={name}
      className={cn(
        "size-full min-w-0 border-t-2",
        DECK_BORDER[deck] ?? "",
        dropActive && "outline-2 -outline-offset-2 outline-accent",
      )}
      onDragOver={onDragOver}
      onDragLeave={() => setDropActive(false)}
      onDrop={onDrop}
    >
      <DeckHeader deck={deck} ready={ready} />
      <DeckOverview deck={deck} label={`${name} position`} className="h-8 shrink-0" />
      <DeckMain deck={deck} ready={ready} />
    </Panel>
  );
}
