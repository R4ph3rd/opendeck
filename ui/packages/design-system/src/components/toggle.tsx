import { cva, type VariantProps } from "class-variance-authority";
import { Toggle as TogglePrimitive, ToggleGroup as GroupPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

export const toggleVariants = cva(
  [
    "inline-flex items-center justify-center gap-2 rounded-md text-sm font-medium text-fg-muted",
    "transition-colors duration-[var(--duration-fast)] ease-standard",
    "hover:bg-surface-hover hover:text-fg",
    "data-[state=on]:bg-accent-muted data-[state=on]:text-accent",
    "disabled:pointer-events-none disabled:opacity-50",
    focusRing,
  ],
  {
    variants: {
      variant: {
        ghost: "",
        outline: "border border-border",
      },
      size: { sm: "h-6 min-w-6 px-2", md: "h-7 min-w-7 px-2", lg: "h-8 min-w-8 px-3" },
    },
    defaultVariants: { variant: "ghost", size: "md" },
  },
);

type ToggleVariantProps = VariantProps<typeof toggleVariants>;

export function Toggle({
  className,
  variant,
  size,
  ...props
}: ComponentProps<typeof TogglePrimitive.Root> & ToggleVariantProps) {
  return (
    <TogglePrimitive.Root className={cn(toggleVariants({ variant, size }), className)} {...props} />
  );
}

export function ToggleGroup({
  className,
  ...props
}: ComponentProps<typeof GroupPrimitive.Root>) {
  return (
    <GroupPrimitive.Root
      className={cn("inline-flex items-center gap-px rounded-md bg-bg-subtle p-px", className)}
      {...props}
    />
  );
}

export function ToggleGroupItem({
  className,
  variant,
  size,
  ...props
}: ComponentProps<typeof GroupPrimitive.Item> & ToggleVariantProps) {
  return (
    <GroupPrimitive.Item className={cn(toggleVariants({ variant, size }), className)} {...props} />
  );
}
