import { cn } from "@opendeck/design-system";
import { useEffect, useRef, type KeyboardEvent, type PointerEvent } from "react";
import { cueToken, deckToken, prefersReducedMotion } from "./colors";
import type { DeckFrameSource } from "./deckFrame";
import {
  KEY_NUDGE_REV,
  TurnAccumulator,
  angleDelta,
  angleOf,
  platterAngle,
  RING_START,
  searchBeats,
  toRevolutions,
  zoneAt,
  type JogZone,
} from "./jogMath";
import { useCanvasLoop } from "./useCanvasLoop";

export interface JogWheelProps {
  deck: number;
  label: string;
  frameSource: DeckFrameSource;
  /** Platter touched (top) or released. Not sent for the outer ring. */
  onTouch: (touched: boolean) => void;
  /** Rotation since the last call, revolutions, clockwise positive. Vinyl mode scratches while touched, otherwise it bends pitch. */
  onTurn: (revolutions: number) => void;
  /** Shift drag: fast search by beats. */
  onSearch: (beats: number) => void;
  /** Main cue point, seconds, drawn as a marker on the ring. */
  cuePoint?: number | null;
  disabled?: boolean;
  className?: string;
}

interface Drag {
  zone: JogZone;
  last: number;
  shift: boolean;
}

/**
 * Jog wheel on a canvas. The platter turns with the playhead at 33 1/3 rpm
 * (still under reduced motion). Press on the platter to touch it (jog_touch)
 * and drag around the centre to send rotation deltas; press on the outer ring
 * to nudge without touching. Shift while dragging does a fast search.
 * Rotation is summed per animation frame, so one command goes out per frame.
 */
export function JogWheel({
  deck,
  label,
  frameSource,
  onTouch,
  onTurn,
  onSearch,
  cuePoint = null,
  disabled = false,
  className,
}: JogWheelProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const acc = useRef(new TurnAccumulator());
  const raf = useRef(0);
  const reduced = useRef(prefersReducedMotion());
  const view = useRef({ cuePoint, disabled, onTouch, onTurn, onSearch });
  const last = useRef("");
  useEffect(() => {
    view.current = { cuePoint, disabled, onTouch, onTurn, onSearch };
  });

  const canvasRef = useCanvasLoop((f) => {
    const { ctx, width, height, dpr, colors } = f;
    const frame = frameSource(f.now);
    const touched = dragRef.current?.zone === "top";
    const key = `${frame.position.toFixed(3)}|${width}|${height}|${touched}|${view.current.cuePoint}|${view.current.disabled}`;
    if (!f.invalidated && key === last.current) return;
    last.current = key;

    ctx.clearRect(0, 0, width, height);
    const cx = width / 2;
    const cy = height / 2;
    const outer = Math.min(width, height) / 2 - dpr;
    const ringW = outer * (1 - RING_START);
    const spin = reduced.current ? 0 : platterAngle(frame.position);
    const at = (seconds: number) => -Math.PI / 2 + platterAngle(seconds) - (reduced.current ? platterAngle(frame.position) : 0);
    ctx.globalAlpha = view.current.disabled ? 0.5 : 1;

    // Outer ring.
    ctx.lineWidth = ringW;
    ctx.strokeStyle = colors.color["surface-hover"];
    ctx.beginPath();
    ctx.arc(cx, cy, outer - ringW / 2, 0, Math.PI * 2);
    ctx.stroke();
    ctx.lineWidth = Math.max(1, dpr * 1.5);
    ctx.strokeStyle = colors.color[deckToken(deck)];
    ctx.beginPath();
    ctx.arc(cx, cy, outer - dpr, 0, Math.PI * 2);
    ctx.stroke();

    // Platter.
    const platter = outer * RING_START;
    ctx.fillStyle = touched ? colors.color["surface-hover"] : colors.color["surface-raised"];
    ctx.beginPath();
    ctx.arc(cx, cy, platter - dpr, 0, Math.PI * 2);
    ctx.fill();
    ctx.lineWidth = Math.max(1, dpr);
    ctx.strokeStyle = colors.color["border"];
    for (const k of [0.55, 0.8]) {
      ctx.beginPath();
      ctx.arc(cx, cy, platter * k, 0, Math.PI * 2);
      ctx.stroke();
    }

    // Spindle and position marker, turning with the playhead.
    const mark = -Math.PI / 2 + spin;
    ctx.strokeStyle = colors.color["fg"];
    ctx.lineWidth = Math.max(2, dpr * 2.5);
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(cx + Math.cos(mark) * platter * 0.2, cy + Math.sin(mark) * platter * 0.2);
    ctx.lineTo(cx + Math.cos(mark) * platter * 0.88, cy + Math.sin(mark) * platter * 0.88);
    ctx.stroke();
    ctx.fillStyle = colors.color["fg-muted"];
    ctx.beginPath();
    ctx.arc(cx, cy, Math.max(2, platter * 0.06), 0, Math.PI * 2);
    ctx.fill();

    // Cue marker on the ring: it meets the position marker when the playhead is on the cue.
    const cue = view.current.cuePoint;
    if (cue !== null) {
      const a = at(cue);
      const r = outer - ringW / 2;
      ctx.fillStyle = colors.color[cueToken(2)];
      ctx.beginPath();
      ctx.arc(cx + Math.cos(a) * r, cy + Math.sin(a) * r, Math.max(3, ringW * 0.36), 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  });

  const flush = () => {
    raf.current = 0;
    const revolutions = acc.current.take();
    const drag = dragRef.current;
    if (revolutions === 0) return;
    if (drag?.shift) view.current.onSearch(searchBeats(revolutions));
    else view.current.onTurn(revolutions);
  };

  const geometry = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const dx = event.clientX - (rect.left + rect.width / 2);
    const dy = event.clientY - (rect.top + rect.height / 2);
    return { dx, dy, radius: Math.min(rect.width, rect.height) / 2 };
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0) return;
    const { dx, dy, radius } = geometry(event);
    const zone = zoneAt(dx, dy, radius);
    if (!zone) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    dragRef.current = { zone, last: angleOf(dx, dy), shift: event.shiftKey };
    if (zone === "top") onTouch(true);
    rootRef.current?.focus({ preventScroll: true });
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    const { dx, dy } = geometry(event);
    // Too close to the spindle the angle is noise.
    if (Math.hypot(dx, dy) < 4) return;
    const a = angleOf(dx, dy);
    acc.current.add(toRevolutions(angleDelta(drag.last, a)));
    drag.last = a;
    drag.shift = event.shiftKey;
    raf.current ||= requestAnimationFrame(flush);
  };
  const end = () => {
    const drag = dragRef.current;
    if (!drag) return;
    if (raf.current) cancelAnimationFrame(raf.current);
    flush();
    dragRef.current = null;
    if (drag.zone === "top") onTouch(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (disabled) return;
    const sign = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (sign !== 0) {
      event.preventDefault();
      onTurn(sign * KEY_NUDGE_REV);
    }
  };

  return (
    <div
      ref={rootRef}
      role="group"
      aria-label={label}
      aria-roledescription="jog wheel"
      tabIndex={disabled ? -1 : 0}
      className={cn(
        "relative aspect-square size-28 touch-none select-none rounded-full outline-offset-2 focus-visible:outline-2 focus-visible:outline-focus-ring",
        disabled ? "opacity-50" : "cursor-grab active:cursor-grabbing",
        className,
      )}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      onKeyDown={onKeyDown}
    >
      <canvas ref={canvasRef} className="block size-full" aria-hidden />
    </div>
  );
}
