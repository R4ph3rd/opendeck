import { Button, IconButton, cn } from "@opendeck/design-system";
import { ChevronLeft, ChevronRight, Eject, FolderOpen, Music } from "lucide-react";
import { useRef } from "react";
import { useStore } from "zustand";
import { useArtworkUrl } from "../../hooks/useArtwork";
import { useTelemetryFrame } from "../../hooks/useTelemetryFrame";
import { DECK_TEXT } from "../../lib/deck";
import { formatClock } from "../../lib/format";
import {
  ejectDeck,
  keySync,
  pickAndLoad,
  setSync,
  stepKeyShift,
  toggleLeader,
  toggleSync,
} from "../../state/actions";
import { selectDeckUi, useAppStore } from "../../state/app";
import {
  selectIsLeader,
  selectLeader,
  selectSyncMode,
  framesToSeconds,
  telemetryStore,
} from "../../state/telemetry";
import { KeyReadout } from "../KeyReadout";
import { DeckButton, DeckToggle } from "./DeckButton";

/** Elapsed and remaining time, written to the DOM once per telemetry snapshot. */
function DeckClock({ deck }: { deck: number }) {
  const elapsedRef = useRef<HTMLSpanElement>(null);
  const remainingRef = useRef<HTMLSpanElement>(null);

  useTelemetryFrame((telemetry) => {
    const d = telemetry?.decks[deck];
    const rate = telemetry?.sample_rate ?? 0;
    const position = d ? framesToSeconds(d.position, rate) : 0;
    const duration = d ? framesToSeconds(d.duration, rate) : 0;
    if (elapsedRef.current) {
      elapsedRef.current.textContent = formatClock(position);
    }
    if (remainingRef.current) {
      remainingRef.current.textContent = formatClock(Math.max(0, duration - position));
    }
  });

  return (
    <div className="flex shrink-0 items-baseline gap-2 font-mono text-sm font-medium leading-none tabular-nums">
      <span
        ref={elapsedRef}
        aria-label={`Deck ${deck + 1} elapsed`}
        data-testid={`elapsed-${deck}`}
      >
        00:00.0
      </span>
      <span
        ref={remainingRef}
        aria-label={`Deck ${deck + 1} remaining`}
        className="text-fg-muted"
        data-testid={`remaining-${deck}`}
      >
        00:00.0
      </span>
    </div>
  );
}

interface DeckHeaderProps {
  deck: number;
  ready: boolean;
}

/**
 * Top row of a deck, one line: number, artwork, title, artist, track BPM and
 * key, elapsed and remaining time, key shift with KEY SYNC, SYNC and MASTER.
 */
export function DeckHeader({ deck, ready }: DeckHeaderProps) {
  const ui = useAppStore(selectDeckUi(deck));
  const audioRunning = useAppStore((s) => s.audio?.running ?? false);
  const requestedLeader = useAppStore((s) => s.leader);
  const reportedLeader = useStore(telemetryStore, selectLeader);
  const reportedIsLeader = useStore(telemetryStore, (s) => selectIsLeader(s, deck));
  const reportedSync = useStore(telemetryStore, (s) => selectSyncMode(s, deck));
  const isLeader = reportedLeader !== null ? reportedIsLeader : requestedLeader === deck;
  const sync = reportedSync ?? ui.sync;
  const track = ui.incoming ?? ui.track;
  const art = useArtworkUrl(track?.id ?? null);
  const name = `Deck ${deck + 1}`;

  return (
    <div className="flex h-[var(--deck-header-h)] shrink-0 items-center gap-2 border-b border-border px-2">
      <span
        className={cn("w-4 shrink-0 text-center text-lg font-bold", DECK_TEXT[deck] ?? "")}
        aria-hidden
      >
        {deck + 1}
      </span>
      <div className="grid size-8 shrink-0 place-items-center overflow-hidden rounded-sm bg-surface-raised text-fg-subtle">
        {art ? (
          <img src={art} alt="" className="size-full object-cover" />
        ) : (
          <Music className="size-4" aria-hidden />
        )}
      </div>
      <div className="min-w-0 flex-1">
        {track ? (
          <>
            <p
              className="truncate text-base font-semibold leading-tight text-fg"
              data-testid={`title-${deck}`}
            >
              {track.title ?? track.file_name}
            </p>
            <p className="flex min-w-0 items-baseline gap-2 text-sm leading-tight text-fg-muted">
              <span className="min-w-0 truncate" data-testid={`artist-${deck}`}>
                {track.artist ?? (ui.loading ? "Loading..." : "Unknown artist")}
              </span>
              <span
                className="shrink-0 font-mono tabular-nums text-fg"
                aria-label={`${name} track BPM`}
                data-testid={`track-bpm-${deck}`}
              >
                {track.bpm ? track.bpm.toFixed(2) : "--"}
              </span>
              <span className="shrink-0 font-mono text-fg" data-testid={`track-key-${deck}`}>
                <KeyReadout deck={deck} />
              </span>
            </p>
          </>
        ) : (
          <div className="flex items-center gap-2">
            <p className="text-sm text-fg-muted">No track loaded</p>
            <Button
              size="sm"
              variant="primary"
              disabled={!audioRunning}
              onClick={() => void pickAndLoad(deck)}
            >
              Open file
            </Button>
          </div>
        )}
      </div>
      <div className="flex shrink-0 flex-col items-end gap-0.5">
      <DeckClock deck={deck} />
      <div className="flex items-center gap-0.5">
        <IconButton
          size="sm"
          className="size-5"
          aria-label={`${name} key down`}
          disabled={!ready}
          onClick={() => stepKeyShift(deck, -1)}
        >
          <ChevronLeft className="size-4" aria-hidden />
        </IconButton>
        <output
          aria-label={`${name} key shift`}
          className="w-7 text-center font-mono text-sm tabular-nums text-fg"
          data-testid={`key-shift-readout-${deck}`}
        >
          {ui.keyShift === 0 ? "±0" : ui.keyShift > 0 ? `+${ui.keyShift}` : String(ui.keyShift)}
        </output>
        <IconButton
          size="sm"
          className="size-5"
          aria-label={`${name} key up`}
          disabled={!ready}
          onClick={() => stepKeyShift(deck, 1)}
        >
          <ChevronRight className="size-4" aria-hidden />
        </IconButton>
        <DeckButton
          className="h-5"
          aria-label={`${name} key sync`}
          disabled={!ready}
          onClick={() => keySync(deck)}
        >
          KEY SYNC
        </DeckButton>
      </div>
      </div>
      <div className="flex w-16 shrink-0 flex-col gap-0.5">
        <DeckToggle
          className="h-5"
          tone="sync"
          aria-label="Sync"
          title="Beat sync. SHIFT-click for tempo-only sync"
          pressed={sync !== "off"}
          disabled={!ready}
          onClick={(event) => {
            if (event.shiftKey) {
              setSync(deck, sync === "tempo" ? "off" : "tempo");
            } else {
              toggleSync(deck);
            }
          }}
          onPressedChange={() => undefined}
        >
          {sync === "tempo" ? "TEMPO SYNC" : "SYNC"}
        </DeckToggle>
        <DeckToggle
          className="h-5"
          tone="master"
          aria-label="Master"
          pressed={isLeader}
          disabled={!ready}
          onPressedChange={() => toggleLeader(deck, isLeader)}
        >
          MASTER
        </DeckToggle>
      </div>
      <div className="flex shrink-0 flex-col">
        <IconButton
          size="sm"
          className="size-5"
          aria-label={`Load track into ${name}`}
          onClick={() => void pickAndLoad(deck)}
        >
          <FolderOpen className="size-4" aria-hidden />
        </IconButton>
        <IconButton
          size="sm"
          className="size-5"
          aria-label={`Eject ${name}`}
          disabled={track === null}
          onClick={() => void ejectDeck(deck)}
        >
          <Eject className="size-4" aria-hidden />
        </IconButton>
      </div>
    </div>
  );
}
