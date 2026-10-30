import { Check } from "lucide-react";
import { RadioGroup } from "radix-ui";
import { cn, focusRing } from "../lib/cn";

export type CueColor = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;

// Static class names so Tailwind generates every cue utility.
const SWATCH: Record<CueColor, string> = {
  1: "bg-cue-1",
  2: "bg-cue-2",
  3: "bg-cue-3",
  4: "bg-cue-4",
  5: "bg-cue-5",
  6: "bg-cue-6",
  7: "bg-cue-7",
  8: "bg-cue-8",
};

const NAMES: Record<CueColor, string> = {
  1: "Pink",
  2: "Blue",
  3: "Amber",
  4: "Violet",
  5: "Green",
  6: "Orange",
  7: "Teal",
  8: "Magenta",
};

export const CUE_COLORS: readonly CueColor[] = [1, 2, 3, 4, 5, 6, 7, 8];

export interface ColorSwatchPickerProps {
  /** Selected cue color (1 to 8), the `--cue-N` token index. */
  value?: CueColor | undefined;
  defaultValue?: CueColor;
  onValueChange?: (value: CueColor) => void;
  /** Subset and order of swatches. Defaults to all eight. */
  colors?: readonly CueColor[];
  disabled?: boolean;
  "aria-label"?: string;
  className?: string;
}

/**
 * Pick one of the hot cue colors. A radio group: arrows move and select, Tab
 * enters and leaves. The selected swatch shows a check, so color is never the
 * only cue.
 */
export function ColorSwatchPicker({
  value,
  defaultValue,
  onValueChange,
  colors = CUE_COLORS,
  disabled,
  className,
  "aria-label": ariaLabel = "Cue color",
}: ColorSwatchPickerProps) {
  return (
    <RadioGroup.Root
      aria-label={ariaLabel}
      orientation="horizontal"
      disabled={disabled}
      {...(value !== undefined ? { value: String(value) } : {})}
      {...(defaultValue !== undefined ? { defaultValue: String(defaultValue) } : {})}
      onValueChange={(v) => onValueChange?.(Number(v) as CueColor)}
      className={cn("inline-flex flex-wrap items-center gap-2", className)}
    >
      {colors.map((c) => (
        <RadioGroup.Item
          key={c}
          value={String(c)}
          aria-label={`${NAMES[c]} (cue ${c})`}
          className={cn(
            "inline-flex size-6 items-center justify-center rounded-full border border-border-strong text-cue-fg",
            "transition-transform duration-[var(--duration-fast)] ease-standard hover:scale-110",
            "data-[state=checked]:outline-2 data-[state=checked]:outline-offset-2 data-[state=checked]:outline-fg",
            "disabled:pointer-events-none disabled:opacity-50",
            SWATCH[c],
            focusRing,
          )}
        >
          <RadioGroup.Indicator>
            <Check className="size-3.5" aria-hidden />
          </RadioGroup.Indicator>
        </RadioGroup.Item>
      ))}
    </RadioGroup.Root>
  );
}
