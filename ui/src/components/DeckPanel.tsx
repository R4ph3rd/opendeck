import {
  Button,
  EmptyState,
  IconButton,
  Panel,
  PanelBody,
  PanelHeader,
} from "@opendeck/design-system";
import { FolderOpen, Music, Pause, Play, Eject } from "lucide-react";
import { useState, type DragEvent, type KeyboardEvent, type PointerEvent } from "react";
import { useStore } from "zustand";
import { cuePress, cueRelease, ejectDeck, pickAndLoad, togglePlay } from "../state/actions";
import { dragKind, readDrag } from "../lib/dnd";
import { DECK_BORDER } from "../lib/deck";
import { loadDroppedFile, loadTrackById } from "../state/libraryActions";
import { selectDeckUi, useAppStore } from "../state/app";
import { telemetryStore } from "../state/telemetry";
import { DeckJog } from "../viz/DeckJog";
import { BeatJumpControls } from "./BeatJumpControls";
import { BpmReadout } from "./BpmReadout";
import { HotCuePads } from "./HotCuePads";
import { KeyReadout } from "./KeyReadout";
import { LoopControls } from "./LoopControls";
import { ProgressBar } from "./ProgressBar";
import { SyncControls } from "./SyncControls";
import { TempoControls } from "./TempoControls";

interface DeckPanelProps {
  deck: number;
}

/** One deck: track info, progress, transport and tempo. */
export function DeckPanel({ deck }: DeckPanelProps) {
  const ui = useAppStore(selectDeckUi(deck));
  const audioRunning = useAppStore((s) => s.audio?.running ?? false);
  // Boolean slices: the component re-renders when these flip, not per tick.
  const playing = useStore(telemetryStore, (s) => s.telemetry?.decks[deck]?.playing ?? false);
  const loaded = useStore(telemetryStore, (s) => s.telemetry?.decks[deck]?.loaded ?? false);

  const track = ui.incoming ?? ui.track;
  const hasTrack = track !== null;
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

  const onCueDown = (event: PointerEvent<HTMLButtonElement>) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    cuePress(deck);
  };
  const onCueUp = (event: PointerEvent<HTMLButtonElement>) => {
    cueRelease(deck);
    // Keep Space free for the global play shortcut after a mouse press.
    event.currentTarget.blur();
  };
  const onCueKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if ((event.key === " " || event.key === "Enter") && !event.repeat) {
      event.preventDefault();
      cuePress(deck);
    }
  };

  return (
    <Panel
      aria-label={name}
      className={`min-w-0 flex-1 border-t-2 ${DECK_BORDER[deck] ?? ""} ${dropActive ? "outline-2 -outline-offset-2 outline-accent" : ""}`}
      onDragOver={onDragOver}
      onDragLeave={() => setDropActive(false)}
      onDrop={onDrop}
    >
      <PanelHeader
        title={name}
        actions={
          <>
            <IconButton
              size="sm"
              aria-label={`Load track into ${name}`}
              onClick={() => void pickAndLoad(deck)}
            >
              <FolderOpen className="size-4" aria-hidden />
            </IconButton>
            <IconButton
              size="sm"
              aria-label={`Eject ${name}`}
              disabled={!hasTrack}
              onClick={() => void ejectDeck(deck)}
            >
              <Eject className="size-4" aria-hidden />
            </IconButton>
          </>
        }
      />
      <PanelBody className="flex gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-3">
          {hasTrack ? (
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="truncate text-lg font-semibold text-fg" data-testid={`title-${deck}`}>
                  {track.title ?? track.file_name}
                </p>
                <p className="truncate text-sm text-fg-muted" data-testid={`artist-${deck}`}>
                  {track.artist ?? (ui.loading ? "Loading..." : "Unknown artist")}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-3">
                <KeyReadout deck={deck} />
                <BpmReadout deck={deck} />
              </div>
            </div>
          ) : (
            <EmptyState
              className="flex-1"
              icon={<Music aria-hidden />}
              title="No track loaded"
              description="Open an audio file to load it into this deck."
              action={
                <Button
                  variant="primary"
                  disabled={!audioRunning}
                  onClick={() => void pickAndLoad(deck)}
                >
                  Open file
                </Button>
              }
            />
          )}
          {hasTrack ? <ProgressBar deck={deck} label={`${name} position`} /> : null}
          <div className="flex items-start gap-3">
            <div className="flex min-w-0 flex-1 flex-col gap-3">
              <div className="flex gap-2">
                <Button
                  variant="primary"
                  size="lg"
                  className="w-24"
                  disabled={!ready}
                  onClick={(event) => {
                    togglePlay(deck);
                    // A mouse click (detail > 0) hands Space back to the global shortcut.
                    if (event.detail > 0) event.currentTarget.blur();
                  }}
                >
                  {playing ? (
                    <Pause className="size-4" aria-hidden />
                  ) : (
                    <Play className="size-4" aria-hidden />
                  )}
                  {playing ? "PAUSE" : "PLAY"}
                </Button>
                <Button
                  size="lg"
                  className="w-24"
                  disabled={!ready}
                  aria-label="Cue"
                  onPointerDown={onCueDown}
                  onPointerUp={onCueUp}
                  onPointerCancel={() => cueRelease(deck)}
                  onLostPointerCapture={() => cueRelease(deck)}
                  onKeyDown={onCueKey}
                  onKeyUp={() => cueRelease(deck)}
                >
                  CUE
                </Button>
              </div>
              <SyncControls deck={deck} disabled={!ready} />
              <LoopControls deck={deck} disabled={!ready} />
              <BeatJumpControls deck={deck} disabled={!ready} />
              <HotCuePads deck={deck} disabled={!ready} />
            </div>
            <DeckJog deck={deck} disabled={!ready} />
          </div>
        </div>
        <TempoControls deck={deck} disabled={!ready} />
      </PanelBody>
    </Panel>
  );
}
