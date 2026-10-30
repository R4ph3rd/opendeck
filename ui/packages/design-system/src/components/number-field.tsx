import { GripVertical } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { cn } from "../lib/cn";

/** Clamp to [min, max] and round to `precision` decimals. */
export function normalizeNumber(value: number, min: number, max: number, precision: number): number {
  const clamped = Math.min(max, Math.max(min, value));
  const f = 10 ** precision;
  return Math.round(clamped * f) / f;
}

export interface NumberFieldProps {
  value?: number;
  defaultValue?: number;
  onValueChange?: (value: number) => void;
  min?: number;
  max?: number;
  /** Arrow key, wheel notch and drag unit. */
  step?: number;
  /** Shift+arrow, PageUp and PageDown. Defaults to `step * 10`. */
  largeStep?: number;
  /** Decimal places shown and kept. Defaults to the places in `step`. */
  precision?: number;
  /** Pixels of horizontal drag per step on the scrub grip. */
  scrubPixels?: number;
  /** Shown after the number, not part of the value (for example "BPM"). */
  unit?: string;
  disabled?: boolean;
  "aria-label"?: string;
  "aria-labelledby"?: string;
  className?: string;
}

function decimals(n: number): number {
  const s = String(n);
  const i = s.indexOf(".");
  return i < 0 ? 0 : s.length - i - 1;
}

/**
 * Numeric entry for BPM and similar. Type and press Enter or blur to commit;
 * ArrowUp/Down step (Shift = large, Alt = fine), PageUp/Down large step,
 * Home/End limits. The mouse wheel steps while the field has focus. Drag the
 * grip sideways to scrub (hold Shift for fine). Exposes role="spinbutton".
 */
export function NumberField({
  value: controlled,
  defaultValue = 0,
  onValueChange,
  min = Number.NEGATIVE_INFINITY,
  max = Number.POSITIVE_INFINITY,
  step = 1,
  largeStep = step * 10,
  precision,
  scrubPixels = 4,
  unit,
  disabled,
  className,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
}: NumberFieldProps) {
  const places = precision ?? decimals(step);
  const [inner, setInner] = useState(() => normalizeNumber(defaultValue, min, max, places));
  const value = controlled ?? inner;
  const [draft, setDraft] = useState<string | null>(null);
  const ref = useRef<HTMLInputElement>(null);
  const latest = useRef({ value, min, max, places, step, onValueChange });
  useEffect(() => {
    latest.current = { value, min, max, places, step, onValueChange };
  });

  function commit(next: number): void {
    const n = normalizeNumber(next, min, max, places);
    if (n !== value) {
      setInner(n);
      onValueChange?.(n);
    }
  }

  function commitDraft(): void {
    if (draft === null) return;
    const parsed = Number(draft.replace(",", "."));
    if (draft.trim() !== "" && Number.isFinite(parsed)) commit(parsed);
    setDraft(null);
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>): void {
    const unitStep = e.altKey ? step / 10 : e.shiftKey ? largeStep : step;
    const base = draft !== null && Number.isFinite(Number(draft)) ? Number(draft) : value;
    let next: number | undefined;
    switch (e.key) {
      case "ArrowUp":
        next = base + unitStep;
        break;
      case "ArrowDown":
        next = base - unitStep;
        break;
      case "PageUp":
        next = base + largeStep;
        break;
      case "PageDown":
        next = base - largeStep;
        break;
      case "Home":
        if (Number.isFinite(min)) next = min;
        break;
      case "End":
        if (Number.isFinite(max)) next = max;
        break;
      case "Enter":
        commitDraft();
        return;
      case "Escape":
        setDraft(null);
        return;
      default:
        return;
    }
    if (next === undefined) return;
    e.preventDefault();
    setDraft(null);
    commit(next);
  }

  // React wheel listeners are passive, so attach natively to be able to preventDefault.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent): void => {
      if (document.activeElement !== el || e.deltaY === 0) return;
      e.preventDefault();
      const c = latest.current;
      const unitStep = e.altKey ? c.step / 10 : c.step;
      const next = c.value + (e.deltaY < 0 ? unitStep : -unitStep);
      const n = normalizeNumber(next, c.min, c.max, c.places);
      if (n !== c.value) {
        setInner(n);
        c.onValueChange?.(n);
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const scrub = useRef<{ x: number; start: number; moved: number } | null>(null);
  function onScrubDown(e: PointerEvent<HTMLSpanElement>): void {
    if (disabled) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    scrub.current = { x: e.clientX, start: value, moved: 0 };
    ref.current?.focus();
  }
  function onScrubMove(e: PointerEvent<HTMLSpanElement>): void {
    const s = scrub.current;
    if (!s) return;
    const unitStep = e.shiftKey ? step / 10 : step;
    const steps = Math.trunc((e.clientX - s.x) / scrubPixels);
    if (steps === s.moved) return;
    s.moved = steps;
    setDraft(null);
    commit(s.start + steps * unitStep);
  }
  function onScrubEnd(e: PointerEvent<HTMLSpanElement>): void {
    scrub.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);
  }

  const shown = draft ?? value.toFixed(places);
  return (
    <div
      data-disabled={disabled ? "" : undefined}
      className={cn(
        "inline-flex h-7 w-28 items-center rounded-md border border-border bg-bg-subtle text-sm text-fg",
        "transition-colors duration-[var(--duration-fast)] ease-standard hover:border-border-strong",
        "focus-within:outline-2 focus-within:outline-offset-1 focus-within:outline-focus-ring",
        "data-[disabled]:opacity-50",
        className,
      )}
    >
      <span
        aria-hidden
        data-number-scrub=""
        className="flex h-full w-4 shrink-0 cursor-ew-resize touch-none items-center justify-center text-fg-subtle hover:text-fg"
        onPointerDown={onScrubDown}
        onPointerMove={onScrubMove}
        onPointerUp={onScrubEnd}
        onPointerCancel={onScrubEnd}
      >
        <GripVertical className="size-3" />
      </span>
      <input
        ref={ref}
        role="spinbutton"
        inputMode="decimal"
        autoComplete="off"
        spellCheck={false}
        disabled={disabled}
        aria-label={ariaLabel}
        aria-labelledby={ariaLabelledBy}
        aria-valuenow={value}
        aria-valuemin={Number.isFinite(min) ? min : undefined}
        aria-valuemax={Number.isFinite(max) ? max : undefined}
        value={shown}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commitDraft}
        onKeyDown={onKeyDown}
        className={cn(
          "h-full min-w-0 flex-1 bg-transparent px-1 text-right font-mono tabular-nums outline-none",
          "disabled:cursor-not-allowed",
        )}
      />
      {unit ? (
        <span aria-hidden className="pr-2 pl-1 text-xs text-fg-muted">
          {unit}
        </span>
      ) : (
        <span aria-hidden className="w-2" />
      )}
    </div>
  );
}
