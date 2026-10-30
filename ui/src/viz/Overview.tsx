import { cn } from "@opendeck/design-system";
import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { DeckFrameSource } from "./deckFrame";
import type { DeckMarkers } from "./markers";
import { createOverviewScratch, drawOverview } from "./overviewDraw";
import type { PeakLevelData } from "./peaks";
import { useCanvasLoop } from "./useCanvasLoop";

export interface OverviewProps {
  label: string;
  /** Whole-track peaks. */
  level: PeakLevelData | null;
  markers: DeckMarkers;
  frameSource: DeckFrameSource;
  /** Click or drag: seek to this position, seconds. */
  onSeek: (seconds: number) => void;
  className?: string;
}

/** Seconds a key press moves the playhead. */
const KEY_SEEK_S = 5;

/** Fraction `0..1` of `clientX` across the element. */
export function fractionAt(clientX: number, rect: { left: number; width: number }): number {
  return rect.width > 0 ? Math.min(1, Math.max(0, (clientX - rect.left) / rect.width)) : 0;
}

/**
 * Full-track overview: click or drag to seek. Role `slider` so it can be used
 * from the keyboard (arrows seek by 5 s, Home and End jump to the ends).
 */
export function Overview({ label, level, markers, frameSource, onSeek, className }: OverviewProps) {
  const [scratch] = useState(createOverviewScratch);
  const scrubRef = useRef<number | null>(null);
  const draggingRef = useRef(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const canvasRef = useCanvasLoop((f) => {
    const deck = frameSource(f.now);
    drawOverview(f, { level, deck, markers, scrub: scrubRef.current }, scratch);
    const root = rootRef.current;
    if (root) {
      const now = String(Math.round(deck.position));
      if (root.getAttribute("aria-valuenow") !== now) {
        root.setAttribute("aria-valuenow", now);
        root.setAttribute("aria-valuemax", String(Math.round(deck.duration)));
        root.setAttribute("aria-valuetext", `${now} of ${Math.round(deck.duration)} seconds`);
      }
    }
  });

  const seekTo = (event: PointerEvent<HTMLDivElement>) => {
    const duration = frameSource(performance.now()).duration;
    if (duration <= 0) {
      return;
    }
    const target = fractionAt(event.clientX, event.currentTarget.getBoundingClientRect()) * duration;
    scrubRef.current = target;
    onSeek(target);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const frame = frameSource(performance.now());
    if (frame.duration <= 0) {
      return;
    }
    const to = (seconds: number) => {
      event.preventDefault();
      onSeek(Math.min(frame.duration, Math.max(0, seconds)));
    };
    if (event.key === "ArrowRight") to(frame.position + KEY_SEEK_S);
    else if (event.key === "ArrowLeft") to(frame.position - KEY_SEEK_S);
    else if (event.key === "Home") to(0);
    else if (event.key === "End") to(frame.duration);
  };

  return (
    <div
      ref={rootRef}
      role="slider"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={0}
      aria-valuenow={0}
      tabIndex={0}
      className={cn(
        "relative h-10 min-w-0 cursor-pointer overflow-hidden rounded-md bg-surface outline-offset-1 focus-visible:outline-2 focus-visible:outline-focus-ring",
        className,
      )}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.currentTarget.setPointerCapture?.(event.pointerId);
        draggingRef.current = true;
        seekTo(event);
      }}
      onPointerMove={(event) => {
        if (draggingRef.current) seekTo(event);
      }}
      onPointerUp={() => {
        draggingRef.current = false;
        setTimeout(() => {
          if (!draggingRef.current) scrubRef.current = null;
        }, 120);
      }}
      onPointerCancel={() => {
        draggingRef.current = false;
        scrubRef.current = null;
      }}
      onKeyDown={onKeyDown}
    >
      <canvas ref={canvasRef} className="block h-full w-full touch-none" aria-hidden />
    </div>
  );
}
