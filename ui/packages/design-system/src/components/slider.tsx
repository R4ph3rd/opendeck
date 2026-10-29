import { Slider as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

/** Horizontal or vertical slider. Thumb count follows the value array length. */
export function Slider({
  className,
  orientation = "horizontal",
  value,
  defaultValue,
  ...props
}: ComponentProps<typeof Primitive.Root>) {
  const thumbs = (value ?? defaultValue ?? [0]).length;
  return (
    <Primitive.Root
      orientation={orientation}
      value={value}
      defaultValue={defaultValue}
      className={cn(
        "relative flex touch-none select-none items-center data-[disabled]:opacity-50",
        "data-[orientation=horizontal]:h-5 data-[orientation=horizontal]:w-full",
        "data-[orientation=vertical]:h-full data-[orientation=vertical]:min-h-24 data-[orientation=vertical]:w-5 data-[orientation=vertical]:flex-col",
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
      {Array.from({ length: thumbs }, (_, i) => (
        <Primitive.Thumb
          key={i}
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
