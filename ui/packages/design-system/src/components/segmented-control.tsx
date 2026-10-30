import { cva, type VariantProps } from "class-variance-authority";
import { RadioGroup } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

const itemVariants = cva(
  [
    "inline-flex items-center justify-center gap-1 rounded-sm text-sm font-medium text-fg-muted",
    "transition-colors duration-[var(--duration-fast)] ease-standard",
    "hover:text-fg data-[state=checked]:bg-surface-raised data-[state=checked]:text-fg data-[state=checked]:shadow-sm",
    "disabled:pointer-events-none disabled:opacity-50",
    focusRing,
  ],
  {
    variants: { size: { sm: "h-5 min-w-6 px-2", md: "h-6 min-w-7 px-3" } },
    defaultVariants: { size: "md" },
  },
);

type Size = NonNullable<VariantProps<typeof itemVariants>["size"]>;

export interface SegmentedOption {
  value: string;
  label: React.ReactNode;
  disabled?: boolean;
  /** Accessible name when `label` is an icon. */
  "aria-label"?: string;
}

export interface SegmentedControlProps
  extends Omit<ComponentProps<typeof RadioGroup.Root>, "children" | "orientation"> {
  options: SegmentedOption[];
  size?: Size;
}

/**
 * Pick exactly one of a few values (tempo range, beat size). A radio group
 * under the hood: arrows move and select. Needs `aria-label` or `aria-labelledby`.
 */
export function SegmentedControl({ options, size, className, ...props }: SegmentedControlProps) {
  return (
    <RadioGroup.Root
      orientation="horizontal"
      className={cn("inline-flex items-center gap-px rounded-md bg-bg-subtle p-px", className)}
      {...props}
    >
      {options.map((o) => (
        <RadioGroup.Item
          key={o.value}
          value={o.value}
          disabled={o.disabled}
          aria-label={o["aria-label"]}
          className={itemVariants({ size })}
        >
          {o.label}
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}
