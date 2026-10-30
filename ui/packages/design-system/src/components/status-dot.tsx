import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

export type StatusTone =
  | "accent"
  | "success"
  | "warning"
  | "danger"
  | "info"
  | "sync"
  | "master"
  | "loop";

// Static class names so Tailwind generates each utility.
const TONE: Record<StatusTone, string> = {
  accent: "bg-accent",
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-danger",
  info: "bg-info",
  sync: "bg-sync-on",
  master: "bg-master-on",
  loop: "bg-loop-edge",
};

export interface StatusDotProps extends Omit<ComponentProps<"span">, "children"> {
  /** `blink` pulses at `--duration-blink` and holds steady under reduced motion. */
  state?: "on" | "off" | "blink";
  tone?: StatusTone;
  /** Text next to the dot (for example "SYNC"). Color is never the only cue. */
  label?: string;
  /** Keep the label for screen readers only. */
  hideLabel?: boolean;
}

/**
 * LED style indicator for SYNC, MASTER, loop and similar. Off shows a hollow
 * ring. The state is exposed as text ("SYNC, on") through a status role.
 */
export function StatusDot({
  state = "on",
  tone = "accent",
  label,
  hideLabel,
  className,
  ...props
}: StatusDotProps) {
  return (
    <span
      role="status"
      data-state={state}
      className={cn("inline-flex items-center gap-1.5 text-xs font-medium", className)}
      {...props}
    >
      <span
        aria-hidden
        className={cn(
          "inline-block size-2 shrink-0 rounded-full border",
          state === "off" ? "border-border-strong bg-transparent" : cn("border-transparent", TONE[tone]),
          state === "blink" &&
            "animate-[ds-blink_var(--duration-blink)_steps(1,end)_infinite] motion-reduce:animate-none",
        )}
      />
      {label ? (
        <span className={cn(state === "off" ? "text-fg-muted" : "text-fg", hideLabel && "sr-only")}>
          {label}
          <span className="sr-only">{state === "off" ? ", off" : ", on"}</span>
        </span>
      ) : (
        <span className="sr-only">{state === "off" ? "off" : "on"}</span>
      )}
    </span>
  );
}
