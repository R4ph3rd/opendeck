import {
  Button,
  EmptyState,
  IconButton,
  Panel,
  PanelBody,
  PanelHeader,
} from "@opendeck/design-system";
import { FolderOpen, Music, Pause, Play, Eject } from "lucide-react";
import type { KeyboardEvent, PointerEvent } from "react";
import { useStore } from "zustand";
import { cuePress, cueRelease, ejectDeck, pickAndLoad, togglePlay } from "../state/actions";
import { selectDeckUi, useAppStore } from "../state/app";
import { telemetryStore } from "../state/telemetry";
import { ProgressBar } from "./ProgressBar";
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
    <Panel aria-label={name} className="border-t-2 border-t-deck-a">
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
            <div className="min-w-0">
              <p className="truncate text-lg font-semibold text-fg" data-testid={`title-${deck}`}>
                {track.title ?? track.file_name}
              </p>
              <p className="truncate text-sm text-fg-muted" data-testid={`artist-${deck}`}>
                {track.artist ?? (ui.loading ? "Loading..." : "Unknown artist")}
              </p>
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
        </div>
        <TempoControls deck={deck} disabled={!ready} />
      </PanelBody>
    </Panel>
  );
}
