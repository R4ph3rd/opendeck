import { Button, Toggle, cn } from "@opendeck/design-system";
import type { ComponentProps } from "react";

/** Colour a lit deck button takes. Classes are whole strings so Tailwind finds them. */
export type Tone = "accent" | "cyan" | "sync" | "master";

const LIT: Record<Tone, string> = {
  accent: "data-[state=on]:text-accent data-[state=on]:shadow-[inset_0_0_0_1px_var(--accent)]",
  cyan: "data-[state=on]:text-accent data-[state=on]:shadow-[inset_0_0_0_1px_var(--accent)]",
  sync: "data-[state=on]:text-sync-on data-[state=on]:shadow-[inset_0_0_0_1px_var(--sync-on)]",
  master: "data-[state=on]:text-master-on data-[state=on]:shadow-[inset_0_0_0_1px_var(--master-on)]",
};

/** Flat grey latching button (SLIP, Q, MT, BEAT SYNC): lit with `tone` when on. */
export function DeckToggle({
  tone = "accent",
  className,
  ...props
}: ComponentProps<typeof Toggle> & { tone?: Tone }) {
  return (
    <Toggle
      className={cn(
        "h-6 min-w-0 shrink-0 rounded-sm bg-surface-raised px-1.5 ds-label",
        "data-[state=on]:bg-surface-raised data-[state=on]:hover:bg-surface-hover",
        LIT[tone],
        className,
      )}
      {...props}
    />
  );
}

/** Flat grey momentary button (loop arrows, IN, OUT). */
export function DeckButton({ className, ...props }: ComponentProps<typeof Button>) {
  return (
    <Button
      size="sm"
      className={cn(
        "min-w-0 rounded-sm bg-surface-raised px-1.5 ds-label hover:bg-surface-hover",
        "border-0",
        className,
      )}
      {...props}
    />
  );
}

/** Round transport button with a coloured ring (CUE accent, PLAY neutral). */
export function RingButton({
  ring,
  className,
  ...props
}: ComponentProps<typeof Button> & { ring: "cue" | "play" }) {
  return (
    <Button
      className={cn(
        "size-[min(100cqh,100cqw)] rounded-full border-4 bg-surface-raised p-0 text-sm font-medium",
        ring === "cue" ? "border-accent" : "border-fg-muted",
        "hover:bg-surface-hover data-[active=true]:bg-surface-hover",
        className,
      )}
      {...props}
    />
  );
}
