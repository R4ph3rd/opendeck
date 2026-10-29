import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

export function Kbd({ className, ...props }: ComponentProps<"kbd">) {
  return (
    <kbd
      className={cn(
        "inline-flex h-4 min-w-4 items-center justify-center rounded-sm border border-border-strong bg-bg-subtle px-1 font-mono text-xs text-fg-muted",
        className,
      )}
      {...props}
    />
  );
}
