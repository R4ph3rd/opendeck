import { cn } from "@opendeck/design-system";
import { useCallback, useEffect, useId, useRef, type KeyboardEvent, type PointerEvent } from "react";
import { dragValue, keyValue, snapToStep, unitToAngle, valueToUnit, wheelValue } from "./knobMath";
import { useCanvasLoop } from "./useCanvasLoop";

export interface KnobProps {
  /** Visible short label. */
  label: string;
  /** Accessible name; defaults to `label`. Include the channel ("Trim A"). */
  name?: string;
  value: number;
  /** Position a double-click returns to. */
  defaultValue: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  /** Text for the readout under the dial and `aria-valuetext`. */
  format?: (value: number) => string;
  /** Arc grows from the centre (EQ, filter). The knob snaps onto the centre. */
  bipolar?: boolean;
  disabled?: boolean;
  className?: string;
}

/** Fine enough that a drag is smooth across the knob's travel. */
const KNOB_STEP = 0.01;

/**
 * Canvas knob. The arc and pointer are drawn from tokens. Interaction: drag
 * vertically (Shift for fine), wheel, arrow / Page / Home / End keys, double
 * click to reset. The accessible control is a visually hidden native range
 * input, so assistive tech gets role slider, `aria-valuenow` and
 * `aria-valuetext` and the browser focuses it normally.
 */
export function Knob({
  label,
  name,
  value,
  defaultValue,
  onChange,
  min = 0,
  max = 1,
  step = KNOB_STEP,
  format = (v) => v.toFixed(2),
  bipolar = false,
  disabled = false,
  className,
}: KnobProps) {
  const id = useId();
  const wrapRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const view = useRef({ value, min, max, step, onChange, disabled, format, bipolar, defaultValue });
  const drag = useRef<{ y: number; start: number } | null>(null);
  const last = useRef("");
  useEffect(() => {
    view.current = { value, min, max, step, onChange, disabled, format, bipolar, defaultValue };
  });

  const canvasRef = useCanvasLoop((f) => {
    const v = view.current;
    const key = `${v.value}|${v.min}|${v.max}|${v.bipolar}|${v.disabled}|${f.width}|${f.height}`;
    if (!f.invalidated && key === last.current) return;
    last.current = key;
    const { ctx, width, height, dpr, colors } = f;
    ctx.clearRect(0, 0, width, height);
    const cx = width / 2;
    const cy = height / 2;
    const stroke = Math.max(2, 3 * dpr);
    const radius = Math.min(width, height) / 2 - stroke;
    const unit = valueToUnit(v.value, v.min, v.max);
    const a0 = unitToAngle(0);
    const a1 = unitToAngle(1);
    ctx.lineCap = "round";
    ctx.lineWidth = stroke;
    ctx.globalAlpha = v.disabled ? 0.4 : 1;

    ctx.strokeStyle = colors.color["surface-hover"];
    ctx.beginPath();
    ctx.arc(cx, cy, radius, a0, a1);
    ctx.stroke();

    const from = v.bipolar ? unitToAngle(0.5) : a0;
    const to = unitToAngle(unit);
    if (Math.abs(to - from) > 0.001) {
      ctx.strokeStyle = colors.color["accent"];
      ctx.beginPath();
      ctx.arc(cx, cy, radius, Math.min(from, to), Math.max(from, to));
      ctx.stroke();
    }

    if (v.bipolar) {
      ctx.strokeStyle = colors.color["fg-subtle"];
      ctx.lineWidth = Math.max(1, dpr);
      const top = unitToAngle(0.5);
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(top) * (radius + stroke * 0.9), cy + Math.sin(top) * (radius + stroke * 0.9));
      ctx.lineTo(cx + Math.cos(top) * (radius + stroke * 1.6), cy + Math.sin(top) * (radius + stroke * 1.6));
      ctx.stroke();
    }

    // Pointer line.
    ctx.strokeStyle = colors.color["fg"];
    ctx.lineWidth = stroke * 0.8;
    ctx.beginPath();
    ctx.moveTo(cx + Math.cos(to) * radius * 0.25, cy + Math.sin(to) * radius * 0.25);
    ctx.lineTo(cx + Math.cos(to) * radius * 0.72, cy + Math.sin(to) * radius * 0.72);
    ctx.stroke();
    ctx.globalAlpha = 1;
  });

  const commit = useCallback((next: number) => {
    const v = view.current;
    const clamped = snapToStep(next, v.min, v.max, 0);
    if (clamped !== v.value) {
      v.onChange(clamped);
    }
  }, []);

  // Wheel needs a non-passive listener to stop the page from scrolling.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return undefined;
    const onWheel = (event: WheelEvent) => {
      const v = view.current;
      if (v.disabled) return;
      event.preventDefault();
      commit(wheelValue(v.value, event.deltaY, v.min, v.max, v.step, event.shiftKey));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [commit]);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (disabled || event.button !== 0) return;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { y: event.clientY, start: value };
    inputRef.current?.focus({ preventScroll: true });
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const v = view.current;
    commit(
      dragValue(d.start, d.y - event.clientY, v.min, v.max, {
        fine: event.shiftKey,
        step: 0,
        bipolar: v.bipolar,
      }),
    );
  };
  const endDrag = () => {
    drag.current = null;
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const next = keyValue(event.key, value, min, max, step, defaultValue);
    if (next !== null) {
      event.preventDefault();
      commit(next);
    }
  };

  return (
    <div className={cn("flex w-full flex-col items-center gap-0.5", className)}>
      <label htmlFor={id} className="text-xs font-medium uppercase text-fg-muted">
        {label}
      </label>
      <div
        ref={wrapRef}
        className={cn(
          "relative size-10 touch-none select-none rounded-full outline-offset-1 focus-within:outline-2 focus-within:outline-focus-ring",
          disabled ? "opacity-50" : "cursor-ns-resize",
        )}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={() => {
          if (!disabled) commit(defaultValue);
        }}
      >
        <canvas ref={canvasRef} className="block size-full" aria-hidden />
        <input
          ref={inputRef}
          id={id}
          type="range"
          aria-label={name ?? label}
          aria-valuetext={format(value)}
          className="pointer-events-none absolute inset-0 size-full opacity-0"
          min={min}
          max={max}
          step={step}
          value={value}
          disabled={disabled}
          onChange={(event) => commit(Number(event.currentTarget.value))}
          onKeyDown={onKeyDown}
        />
      </div>
      <output htmlFor={id} className="font-mono text-xs tabular-nums text-fg-subtle">
        {format(value)}
      </output>
    </div>
  );
}
