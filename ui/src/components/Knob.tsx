import { useId } from "react";

interface KnobProps {
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
  /** Text for the readout and `aria-valuetext`. */
  format?: (value: number) => string;
  disabled?: boolean;
}

/** Fine enough that a drag is smooth across the knob's travel. */
const KNOB_STEP = 0.01;

/**
 * A knob as an accessible range input styled with tokens. ui-viz replaces it
 * with a canvas knob in M4; the props stay the same. Double-click resets.
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
  disabled,
}: KnobProps) {
  const id = useId();
  return (
    <div className="flex w-full flex-col items-center gap-1">
      <label htmlFor={id} className="text-xs font-medium uppercase text-fg-muted">
        {label}
      </label>
      <input
        id={id}
        type="range"
        aria-label={name ?? label}
        aria-valuetext={format(value)}
        className="h-4 w-full cursor-pointer accent-accent focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus-ring disabled:opacity-50"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
        onDoubleClick={() => onChange(defaultValue)}
      />
      <output htmlFor={id} className="font-mono text-xs tabular-nums text-fg-subtle">
        {format(value)}
      </output>
    </div>
  );
}
