import { cn } from "@opendeck/design-system";
import { useRef, useState, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import type { TokenName } from "./colors";

export interface PadSpec {
  /** Text on the pad, usually its number. */
  label: string;
  /** Colour token of the pad when lit. `null` uses the neutral accent. */
  color: TokenName | null;
  /** Lit: the slot holds something (a stored cue, an active loop). */
  lit: boolean;
  /** Accessible name; defaults to `label`. */
  ariaLabel?: string;
  disabled?: boolean;
}

export interface PerformancePadsProps {
  /** Group name for assistive tech, for example "Hot cues". */
  label: string;
  pads: readonly PadSpec[];
  /** Pointer or key went down on a pad. */
  onPress: (index: number, modifiers: { shift: boolean }) => void;
  /** Pointer or key came up. Called once per press, also when the pointer is lost. */
  onRelease?: (index: number) => void;
  /** Pads per row. Eight pads at 4 give the usual 2 x 4 grid. */
  columns?: 2 | 4 | 8;
  /** Disables every pad. */
  disabled?: boolean;
  className?: string;
}

const GRID = { 2: "grid-cols-2", 4: "grid-cols-4", 8: "grid-cols-8" } as const;

const token = (name: TokenName): string => `var(--${name})`;

/**
 * Grid of velocity-less performance pads, generic over what they do: hot
 * cues now, loops, rolls and effects when pad modes arrive. A lit pad is
 * filled with its colour token; a pressed pad is inset and brighter. Press
 * and release are reported once each, including when the pointer is lost.
 */
export function PerformancePads({
  label,
  pads,
  onPress,
  onRelease,
  columns = 4,
  disabled = false,
  className,
}: PerformancePadsProps) {
  const [pressed, setPressed] = useState<ReadonlySet<number>>(new Set());
  const held = useRef(new Set<number>());

  const down = (index: number, shift: boolean) => {
    if (held.current.has(index)) return;
    held.current.add(index);
    setPressed(new Set(held.current));
    onPress(index, { shift });
  };
  const up = (index: number) => {
    if (!held.current.delete(index)) return;
    setPressed(new Set(held.current));
    onRelease?.(index);
  };

  return (
    <div role="group" aria-label={label} className={cn("grid gap-1", GRID[columns], className)}>
      {pads.map((pad, index) => {
        const off = disabled || pad.disabled;
        const color = token(pad.color ?? "accent");
        const isPressed = pressed.has(index);
        const style: CSSProperties = pad.lit
          ? { backgroundColor: color, borderColor: "transparent", color: token("cue-fg") }
          : { borderColor: "var(--border)", color };
        return (
          <button
            key={index}
            type="button"
            aria-label={pad.ariaLabel ?? pad.label}
            aria-pressed={pad.lit}
            data-pressed={isPressed || undefined}
            disabled={off}
            style={style}
            className={cn(
              "h-9 min-w-0 select-none rounded-md border text-sm font-semibold tabular-nums",
              "transition-[filter,transform] duration-[var(--duration-fast)] ease-standard",
              "outline-offset-1 focus-visible:outline-2 focus-visible:outline-focus-ring",
              "disabled:pointer-events-none disabled:opacity-50",
              "hover:brightness-110 data-[pressed]:translate-y-px data-[pressed]:brightness-125",
              !pad.lit && "bg-surface-raised",
            )}
            onPointerDown={(event: PointerEvent<HTMLButtonElement>) => {
              if (event.button !== 0) return;
              event.currentTarget.setPointerCapture?.(event.pointerId);
              down(index, event.shiftKey);
            }}
            onPointerUp={() => up(index)}
            onPointerCancel={() => up(index)}
            onLostPointerCapture={() => up(index)}
            onKeyDown={(event: KeyboardEvent<HTMLButtonElement>) => {
              if ((event.key === " " || event.key === "Enter") && !event.repeat) {
                event.preventDefault();
                down(index, event.shiftKey);
              }
            }}
            onKeyUp={(event: KeyboardEvent<HTMLButtonElement>) => {
              if (event.key === " " || event.key === "Enter") up(index);
            }}
            onBlur={() => up(index)}
          >
            {pad.label}
          </button>
        );
      })}
    </div>
  );
}
