import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

/**
 * Loading placeholder. Size it with classes (`h-4 w-32`). Decorative: hide it
 * from assistive tech and announce loading on the container (`aria-busy`).
 */
export function Skeleton({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      aria-hidden
      className={cn(
        "rounded-md bg-surface-hover animate-[ds-pulse_var(--duration-sweep)_ease-in-out_infinite] motion-reduce:animate-none",
        className,
      )}
      {...props}
    />
  );
}
