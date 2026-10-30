import { HoverCard as HoverPrimitive, Popover as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

const floating =
  "z-[var(--z-popover)] rounded-md border border-border bg-surface-raised p-3 text-sm text-fg shadow-md outline-none animate-[ds-pop-in_var(--duration-fast)_var(--ease-standard)]";

export const Popover = Primitive.Root;
export const PopoverTrigger = Primitive.Trigger;
export const PopoverAnchor = Primitive.Anchor;
export const PopoverClose = Primitive.Close;

/** Click-opened floating panel. Focus moves in, Escape closes and returns focus. */
export function PopoverContent({
  className,
  sideOffset = 6,
  align = "center",
  ...props
}: ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Primitive.Content
        sideOffset={sideOffset}
        align={align}
        className={cn(floating, "w-72", className)}
        {...props}
      />
    </Primitive.Portal>
  );
}

export const HoverCard = HoverPrimitive.Root;
export const HoverCardTrigger = HoverPrimitive.Trigger;

/**
 * Hover and focus preview (track details, cue info). Never put the only copy of
 * an action in it.
 */
export function HoverCardContent({
  className,
  sideOffset = 6,
  align = "center",
  ...props
}: ComponentProps<typeof HoverPrimitive.Content>) {
  return (
    <HoverPrimitive.Portal>
      <HoverPrimitive.Content
        sideOffset={sideOffset}
        align={align}
        className={cn(floating, "w-64", className)}
        {...props}
      />
    </HoverPrimitive.Portal>
  );
}
