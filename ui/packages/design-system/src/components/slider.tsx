import { Slider as Primitive } from "radix-ui";
import type { ComponentProps, CSSProperties } from "react";
import { cn } from "../lib/cn";

export interface SliderProps extends ComponentProps<typeof Primitive.Root> {
  /** Vertical track length as any CSS length. Defaults to the `--slider-track-length` token. */
  trackLength?: string;
  /** Draws a tick on the track: a value, or "center" for the midpoint (tempo fader zero). */
  detent?: number | "center";
  /** Accessible text for the current value, applied to every thumb. */
  "aria-valuetext"?: string;
}

/**
 * Horizontal or vertical slider. Thumb count follows the value array length.
 * `aria-label`, `aria-labelledby` and `aria-valuetext` go to each thumb, the
 * element that carries role="slider".
 */
export function Slider({
  className,
  orientation = "horizontal",
  value,
  defaultValue,
  trackLength,
  detent,
  min = 0,
  max = 100,
  style,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  "aria-valuetext": ariaValueText,
  ...props
}: SliderProps) {
  const thumbs = (value ?? defaultValue ?? [min]).length;
  const vertical = orientation === "vertical";
  const detentValue = detent === "center" ? (min + max) / 2 : detent;
  const detentPct =
    detentValue === undefined || max === min ? undefined : ((detentValue - min) / (max - min)) * 100;
  const rootStyle = (
    trackLength ? { ...style, "--slider-length": trackLength } : style
  ) as CSSProperties | undefined;

  return (
    <Primitive.Root
      orientation={orientation}
      value={value}
      defaultValue={defaultValue}
      min={min}
      max={max}
      style={rootStyle}
      className={cn(
        "relative flex touch-none select-none items-center data-[disabled]:opacity-50",
        "data-[orientation=horizontal]:h-5 data-[orientation=horizontal]:w-full",
        "data-[orientation=vertical]:h-[var(--slider-length,var(--slider-track-length))] data-[orientation=vertical]:w-5 data-[orientation=vertical]:flex-col",
        className,
      )}
      {...props}
    >
      <Primitive.Track
        className={cn(
          "relative grow overflow-hidden rounded-full bg-surface-hover",
          "data-[orientation=horizontal]:h-1 data-[orientation=vertical]:w-1",
        )}
      >
        <Primitive.Range
          className={cn(
            "absolute bg-accent",
            "data-[orientation=horizontal]:h-full data-[orientation=vertical]:w-full",
          )}
        />
      </Primitive.Track>
      {detentPct === undefined ? null : (
        <span
          aria-hidden
          data-slider-detent=""
          className={cn(
            "pointer-events-none absolute bg-border-strong",
            vertical ? "left-1/2 h-px w-3 -translate-x-1/2" : "top-1/2 h-3 w-px -translate-y-1/2",
          )}
          style={vertical ? { bottom: `${detentPct}%` } : { left: `${detentPct}%` }}
        />
      )}
      {Array.from({ length: thumbs }, (_, i) => (
        <Primitive.Thumb
          key={i}
          aria-label={ariaLabel}
          aria-labelledby={ariaLabelledBy}
          aria-valuetext={ariaValueText}
          className={cn(
            "block size-4 rounded-full border border-border-strong bg-fg shadow-sm",
            "transition-colors duration-[var(--duration-fast)] ease-standard hover:bg-accent-hover",
            "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus-ring",
          )}
        />
      ))}
    </Primitive.Root>
  );
}
