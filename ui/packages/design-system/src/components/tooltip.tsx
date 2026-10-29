import { Tooltip as Primitive } from "radix-ui";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "../lib/cn";
import { Kbd } from "./kbd";

export const TooltipProvider = Primitive.Provider;
export const TooltipTrigger = Primitive.Trigger;
export const TooltipRoot = Primitive.Root;

export function TooltipContent({
  className,
  sideOffset = 6,
  ...props
}: ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Primitive.Content
        sideOffset={sideOffset}
        className={cn(
          "z-[var(--z-tooltip)] flex items-center gap-2 rounded-md border border-border bg-surface-raised px-2 py-1 text-xs text-fg shadow-md",
          "animate-[ds-pop-in_var(--duration-fast)_var(--ease-standard)]",
          className,
        )}
        {...props}
      />
    </Primitive.Portal>
  );
}

export interface TooltipProps {
  content: ReactNode;
  /** Optional shortcut hint shown as a Kbd. */
  shortcut?: string;
  side?: "top" | "right" | "bottom" | "left";
  children: ReactNode;
}

/** One-liner tooltip. Mounts its own provider; share one via TooltipProvider for delay settings. */
export function Tooltip({ content, shortcut, side = "top", children }: TooltipProps) {
  return (
    <Primitive.Provider delayDuration={400}>
      <Primitive.Root>
        <Primitive.Trigger asChild>{children}</Primitive.Trigger>
        <TooltipContent side={side}>
          {content}
          {shortcut ? <Kbd>{shortcut}</Kbd> : null}
        </TooltipContent>
      </Primitive.Root>
    </Primitive.Provider>
  );
}
