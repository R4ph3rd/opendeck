import { Progress as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

export interface ProgressBarProps
  extends Omit<ComponentProps<typeof Primitive.Root>, "value" | "children"> {
  /** 0 to `max`. Omit or pass `null` for indeterminate (unknown duration). */
  value?: number | null;
  /** Needs `aria-label` or `aria-labelledby`. */
  "aria-label"?: string;
}

/** Determinate or indeterminate bar, for imports and analysis. */
export function ProgressBar({ value = null, max = 100, className, ...props }: ProgressBarProps) {
  const pct = value === null ? 0 : Math.min(100, Math.max(0, (value / max) * 100));
  return (
    <Primitive.Root
      value={value}
      max={max}
      className={cn("relative h-1.5 w-full overflow-hidden rounded-full bg-surface-hover", className)}
      {...props}
    >
      {value === null ? (
        <Primitive.Indicator
          className="absolute inset-y-0 left-0 w-2/5 rounded-full bg-accent animate-[ds-sweep_var(--duration-sweep)_var(--ease-standard)_infinite] motion-reduce:w-full motion-reduce:animate-[ds-pulse_var(--duration-sweep)_ease-in-out_infinite]"
        />
      ) : (
        <Primitive.Indicator
          className="size-full rounded-full bg-accent transition-transform duration-[var(--duration-base)] ease-standard"
          style={{ transform: `translateX(-${100 - pct}%)` }}
        />
      )}
    </Primitive.Root>
  );
}
