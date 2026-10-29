import { ScrollArea as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

export function ScrollArea({
  className,
  children,
  ...props
}: ComponentProps<typeof Primitive.Root>) {
  return (
    <Primitive.Root className={cn("relative overflow-hidden", className)} {...props}>
      <Primitive.Viewport className={cn("size-full rounded-[inherit]", focusRing)}>
        {children}
      </Primitive.Viewport>
      <ScrollBar />
      <ScrollBar orientation="horizontal" />
      <Primitive.Corner />
    </Primitive.Root>
  );
}

export function ScrollBar({
  className,
  orientation = "vertical",
  ...props
}: ComponentProps<typeof Primitive.Scrollbar>) {
  return (
    <Primitive.Scrollbar
      orientation={orientation}
      className={cn(
        "flex touch-none select-none p-px",
        orientation === "vertical" && "h-full w-2",
        orientation === "horizontal" && "h-2 flex-col",
        className,
      )}
      {...props}
    >
      <Primitive.Thumb className="relative flex-1 rounded-full bg-[var(--scrollbar-thumb)] hover:bg-[var(--scrollbar-thumb-hover)]" />
    </Primitive.Scrollbar>
  );
}
