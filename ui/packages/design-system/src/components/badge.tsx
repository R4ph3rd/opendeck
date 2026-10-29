import { cva, type VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

export const badgeVariants = cva(
  "inline-flex h-4 items-center gap-1 rounded-sm border px-1.5 text-xs font-medium",
  {
    variants: {
      variant: {
        neutral: "border-border bg-surface-hover text-fg-muted",
        accent: "border-transparent bg-accent-muted text-accent",
        success: "border-transparent bg-surface-hover text-success",
        warning: "border-transparent bg-surface-hover text-warning",
        danger: "border-transparent bg-surface-hover text-danger",
      },
    },
    defaultVariants: { variant: "neutral" },
  },
);

export function Badge({
  className,
  variant,
  ...props
}: ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
