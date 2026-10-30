import { IconButton, cn } from "@opendeck/design-system";
import { ZoomIn, ZoomOut } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { DeckFrameSource } from "./deckFrame";
import type { DeckMarkers } from "./markers";
import type { PeakLevelData } from "./peaks";
import { useCanvasLoop } from "./useCanvasLoop";
import { createWaveformScratch, drawWaveform } from "./waveformDraw";
import { ZOOM_SECONDS, scrubDelta, stepZoom } from "./waveformMath";

export interface WaveformProps {
  /** Accessible name, for example "Deck A waveform". */
  label: string;
  /** Detail peak levels (any order, `null` while loading). */
  levels: readonly (PeakLevelData | null)[];
  markers: DeckMarkers;
  /** Per-frame playhead and loop state. Must be stable across renders. */
  frameSource: DeckFrameSource;
  zoomIndex: number;
  onZoomChange: (index: number) => void;
  /** Seeks while dragging, seconds. Called at most once per animation frame. */
  onScrub?: (seconds: number) => void;
  /** Drag started or ended; for engines that want a touch signal. */
  onScrubActive?: (active: boolean) => void;
  className?: string;
}

/** Wheel distance that counts as one zoom step. */
const WHEEL_STEP = 60;

/**
 * Zoomed, centred, scrolling waveform on a 2D canvas. The playhead is
 * extrapolated from telemetry every animation frame (see `interp.ts`), so it
 * stays smooth at any rate. Drag to scrub, wheel or +/- to zoom. Renders
 * nothing through React per frame.
 */
export function Waveform({
  label,
  levels,
  markers,
  frameSource,
  zoomIndex,
  onZoomChange,
  onScrub,
  onScrubActive,
  className,
}: WaveformProps) {
  const [scratch] = useState(createWaveformScratch);
  const scrubRef = useRef<number | null>(null);
  const dragRef = useRef<{ x: number; start: number; width: number } | null>(null);
  const pendingRef = useRef<number | null>(null);
  const frameRef = useRef(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const zoomRef = useRef(zoomIndex);
  const zoomHandler = useRef(onZoomChange);
  useEffect(() => {
    zoomRef.current = zoomIndex;
    zoomHandler.current = onZoomChange;
  });

  const canvasRef = useCanvasLoop((f) => {
    const deck = frameSource(f.now);
    drawWaveform(f, { levels, deck, markers, zoomIndex, scrub: scrubRef.current }, scratch);
  });

  // React attaches wheel listeners as passive; zooming needs preventDefault.
  useEffect(() => {
    const el = containerRef.current;
    if (!el) {
      return undefined;
    }
    let acc = 0;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      acc += event.deltaY;
      const steps = Math.trunc(acc / WHEEL_STEP);
      if (steps !== 0) {
        acc -= steps * WHEEL_STEP;
        let index = zoomRef.current;
        // Wheel up (negative) zooms in.
        for (let i = 0; i < Math.abs(steps); i += 1) index = stepZoom(index, steps < 0 ? 1 : -1);
        zoomRef.current = index;
        zoomHandler.current(index);
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const flush = () => {
    frameRef.current = 0;
    if (pendingRef.current !== null) {
      onScrub?.(pendingRef.current);
      pendingRef.current = null;
    }
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target as HTMLElement).closest("button")) {
      return;
    }
    const frame = frameSource(performance.now());
    if (!frame.loaded) {
      return;
    }
    event.currentTarget.setPointerCapture?.(event.pointerId);
    dragRef.current = {
      x: event.clientX,
      start: frame.position,
      width: event.currentTarget.clientWidth,
    };
    onScrubActive?.(true);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) {
      return;
    }
    const visible = ZOOM_SECONDS[zoomIndex] ?? ZOOM_SECONDS[2];
    const duration = frameSource(performance.now()).duration;
    const target = Math.min(
      duration,
      Math.max(0, drag.start + scrubDelta(event.clientX - drag.x, drag.width, visible)),
    );
    scrubRef.current = target;
    pendingRef.current = target;
    frameRef.current ||= requestAnimationFrame(flush);
  };
  const endDrag = () => {
    if (!dragRef.current) {
      return;
    }
    dragRef.current = null;
    if (frameRef.current) {
      cancelAnimationFrame(frameRef.current);
    }
    flush();
    // Hold the scrubbed position until telemetry catches up with the seek.
    setTimeout(() => {
      if (!dragRef.current) scrubRef.current = null;
    }, 120);
    onScrubActive?.(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "+" || event.key === "=") {
      onZoomChange(stepZoom(zoomIndex, 1));
      event.preventDefault();
    } else if (event.key === "-" || event.key === "_") {
      onZoomChange(stepZoom(zoomIndex, -1));
      event.preventDefault();
    }
  };

  const seconds = ZOOM_SECONDS[zoomIndex] ?? ZOOM_SECONDS[2];
  return (
    <div
      ref={containerRef}
      role="group"
      aria-label={label}
      tabIndex={0}
      className={cn(
        "group relative h-20 min-w-0 cursor-grab overflow-hidden rounded-md bg-surface outline-offset-1 focus-visible:outline-2 focus-visible:outline-focus-ring active:cursor-grabbing",
        className,
      )}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onKeyDown={onKeyDown}
    >
      <canvas ref={canvasRef} className="block h-full w-full touch-none" aria-hidden />
      <div className="pointer-events-none absolute right-1 top-1 flex items-center gap-0.5 rounded-md bg-surface-raised/80 px-1">
        <span className="font-mono text-xs tabular-nums text-fg-muted">{seconds}s</span>
        <IconButton
          size="sm"
          className="pointer-events-auto"
          aria-label={`Zoom in ${label}`}
          disabled={zoomIndex <= 0}
          onClick={() => onZoomChange(stepZoom(zoomIndex, 1))}
        >
          <ZoomIn className="size-3.5" aria-hidden />
        </IconButton>
        <IconButton
          size="sm"
          className="pointer-events-auto"
          aria-label={`Zoom out ${label}`}
          disabled={zoomIndex >= ZOOM_SECONDS.length - 1}
          onClick={() => onZoomChange(stepZoom(zoomIndex, -1))}
        >
          <ZoomOut className="size-3.5" aria-hidden />
        </IconButton>
      </div>
    </div>
  );
}
