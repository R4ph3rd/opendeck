import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

/**
 * Dense table primitives, unopinionated about state so they can render the output of
 * TanStack Table (flexRender into Th/Td) and TanStack Virtual (spacer rows).
 */
export function Table({ className, ...props }: ComponentProps<"table">) {
  return (
    <table
      className={cn("w-full border-collapse text-sm tabular-nums text-fg", className)}
      {...props}
    />
  );
}

export function TableHeader({ className, ...props }: ComponentProps<"thead">) {
  return (
    <thead
      className={cn("sticky top-0 z-[var(--z-sticky)] bg-panel-header", className)}
      {...props}
    />
  );
}

export function TableBody({ className, ...props }: ComponentProps<"tbody">) {
  return <tbody className={className} {...props} />;
}

export function TableRow({ className, ...props }: ComponentProps<"tr">) {
  return (
    <tr
      className={cn(
        "h-[var(--row-h)] border-b border-border transition-colors duration-[var(--duration-fast)] ease-standard",
        "hover:bg-surface-hover aria-selected:bg-accent-muted data-[state=selected]:bg-accent-muted",
        "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus-ring",
        className,
      )}
      {...props}
    />
  );
}

export function TableHead({ className, ...props }: ComponentProps<"th">) {
  return (
    <th
      className={cn(
        "h-[var(--row-h)] select-none border-b border-border-strong px-2 text-left text-xs font-medium text-fg-muted",
        "aria-sort:text-fg",
        focusRing,
        className,
      )}
      {...props}
    />
  );
}

export function TableCell({ className, ...props }: ComponentProps<"td">) {
  return <td className={cn("truncate px-2 align-middle", className)} {...props} />;
}
