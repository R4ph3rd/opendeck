import { Switch as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

export function Switch({ className, ...props }: ComponentProps<typeof Primitive.Root>) {
  return (
    <Primitive.Root
      className={cn(
        "peer inline-flex h-4 w-7 shrink-0 items-center rounded-full border border-border-strong bg-surface-hover p-px",
        "transition-colors duration-[var(--duration-fast)] ease-standard",
        "data-[state=checked]:border-accent data-[state=checked]:bg-accent",
        "disabled:cursor-not-allowed disabled:opacity-50",
        focusRing,
        className,
      )}
      {...props}
    >
      <Primitive.Thumb
        className={cn(
          "block size-3 rounded-full bg-fg shadow-sm",
          "transition-transform duration-[var(--duration-fast)] ease-standard",
          "data-[state=checked]:translate-x-3 data-[state=checked]:bg-accent-fg",
        )}
      />
    </Primitive.Root>
  );
}
