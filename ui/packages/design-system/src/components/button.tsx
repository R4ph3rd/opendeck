import { cva, type VariantProps } from "class-variance-authority";
import { Loader2 } from "lucide-react";
import { Slot } from "radix-ui";
import type { ButtonHTMLAttributes, Ref } from "react";
import { cn, focusRing } from "../lib/cn";

export const buttonVariants = cva(
  [
    "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-md font-medium",
    "transition-colors duration-[var(--duration-fast)] ease-standard",
    "disabled:pointer-events-none disabled:opacity-50",
    focusRing,
  ],
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-fg hover:bg-accent-hover",
        secondary: "border border-border bg-surface-raised text-fg hover:bg-surface-hover",
        ghost: "text-fg-muted hover:bg-surface-hover hover:text-fg",
        outline: "border border-border-strong text-fg hover:bg-surface-hover",
        danger: "bg-danger text-danger-fg hover:brightness-110",
      },
      size: {
        sm: "h-6 px-2 text-xs",
        md: "h-7 px-3 text-sm",
        lg: "h-8 px-4 text-base",
      },
    },
    defaultVariants: { variant: "secondary", size: "md" },
  },
);

export interface ButtonProps
  extends ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
  loading?: boolean;
  ref?: Ref<HTMLButtonElement>;
}

export function Button({
  className,
  variant,
  size,
  asChild = false,
  loading = false,
  disabled,
  children,
  ref,
  ...props
}: ButtonProps) {
  const Comp = asChild ? Slot.Root : "button";
  return (
    <Comp
      ref={ref}
      className={cn(buttonVariants({ variant, size }), className)}
      disabled={disabled === true || loading}
      aria-busy={loading || undefined}
      {...props}
    >
      {asChild ? (
        children
      ) : (
        <>
          {loading ? <Loader2 className="size-3.5 animate-spin" aria-hidden /> : null}
          {children}
        </>
      )}
    </Comp>
  );
}
