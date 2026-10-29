import { cva, type VariantProps } from "class-variance-authority";
import type { ButtonHTMLAttributes, Ref } from "react";
import { cn, focusRing } from "../lib/cn";

const iconButtonVariants = cva(
  [
    "inline-flex shrink-0 items-center justify-center rounded-md text-fg-muted",
    "transition-colors duration-[var(--duration-fast)] ease-standard",
    "hover:bg-surface-hover hover:text-fg",
    "disabled:pointer-events-none disabled:opacity-50",
    "aria-pressed:bg-accent-muted aria-pressed:text-accent",
    focusRing,
  ],
  {
    variants: {
      size: { sm: "size-6", md: "size-7", lg: "size-8" },
    },
    defaultVariants: { size: "md" },
  },
);

export interface IconButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label">,
    VariantProps<typeof iconButtonVariants> {
  /** Required: an icon-only control needs an accessible name. */
  "aria-label": string;
  ref?: Ref<HTMLButtonElement>;
}

export function IconButton({ className, size, type = "button", ref, ...props }: IconButtonProps) {
  return (
    <button
      ref={ref}
      type={type}
      className={cn(iconButtonVariants({ size }), className)}
      {...props}
    />
  );
}
