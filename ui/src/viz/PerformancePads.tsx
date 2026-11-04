import { ContextMenu, ContextMenuContent, ContextMenuTrigger, cn } from "@opendeck/design-system";
import {
  useRef,
  useState,
  type CSSProperties,
  type HTMLAttributes,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from "react";
import type { TokenName } from "./colors";

export interface PadSpec {
  /** Main text on the pad: a cue name, an effect, a jump size. */
  label: string;
  /** Second line, small mono text: a time, a division, a length. */
  sublabel?: string | undefined;
  /** Colour token of the pad when lit. `null` uses the neutral accent. */
  color: TokenName | null;
  /** Lit: the slot holds something (a stored cue, an active loop). */
  lit: boolean;
  /** Sounding right now (a playing sample): drawn with an inset ring. */
  active?: boolean;
  /** Accessible name; defaults to `label`. */
  ariaLabel?: string;
  disabled?: boolean;
}

export interface PerformancePadsProps {
  /** Group name for assistive tech, for example "Hot cues". */
  label: string;
  pads: readonly PadSpec[];
  /** Pointer or key went down on a pad. `shift` is the SHIFT key at that moment. */
  onPress: (index: number, modifiers: { shift: boolean }) => void;
  /** Pointer or key came up. Called once per press, also when the pointer is lost. */
  onRelease?: (index: number) => void;
  /** Pads per row. Eight pads at 4 give the usual 2 x 4 grid. */
  columns?: 2 | 4 | 8;
  /** Disables every pad. */
  disabled?: boolean;
  /** Content of the right-click menu of a pad (menu items). No menu when absent or `null`. */
  renderMenu?: (index: number) => ReactNode;
  /** Extra props per pad, for example drop handlers. */
  padProps?: (index: number) => HTMLAttributes<HTMLButtonElement>;
  className?: string;
}

const GRID = {
  2: "grid-cols-2 grid-rows-4",
  4: "grid-cols-4 grid-rows-2",
  8: "grid-cols-8 grid-rows-1",
} as const;

const token = (name: TokenName): string => `var(--${name})`;

/**
 * Grid of velocity-less performance pads that fills its container: eight flat
 * grey rectangles, each lit with its colour token when set. A pressed pad is
 * brighter and inset. Press and release are reported once each, including
 * when the pointer is lost. SHIFT while pressing is reported to the caller.
 */
export function PerformancePads({
  label,
  pads,
  onPress,
  onRelease,
  columns = 4,
  disabled = false,
  renderMenu,
  padProps,
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
    <div
      role="group"
      aria-label={label}
      className={cn("grid size-full min-h-0 gap-0.5", GRID[columns], className)}
    >
      {pads.map((pad, index) => {
        const off = disabled || pad.disabled;
        const color = token(pad.color ?? "accent");
        const isPressed = pressed.has(index);
        // Neutral grey pad; a set slot shows a thin muted edge in its colour token.
        const style: CSSProperties = pad.lit
          ? { boxShadow: `inset 3px 0 0 ${color}` }
          : {};
        const button = (
          <button
            key={index}
            type="button"
            aria-label={pad.ariaLabel ?? pad.label}
            aria-pressed={pad.lit}
            data-pressed={isPressed || undefined}
            disabled={off}
            style={style}
            className={cn(
              "relative flex min-h-0 min-w-0 select-none flex-col items-center justify-center gap-0.5 overflow-hidden px-1",
              "transition-[filter,transform] duration-[var(--duration-fast)] ease-standard",
              "-outline-offset-2 focus-visible:outline-2 focus-visible:outline-focus-ring",
              "disabled:pointer-events-none disabled:opacity-50",
              "hover:bg-surface-hover data-[pressed]:translate-y-px data-[pressed]:bg-surface-hover",
              "bg-surface-raised text-fg",
              pad.active && "bg-surface-hover shadow-[inset_0_0_0_1px_var(--accent)]",
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
            {...padProps?.(index)}
          >
            <span
              aria-hidden
              className={cn(
                "absolute left-1.5 top-1 font-mono text-xs tabular-nums",
                "text-fg-subtle",
              )}
            >
              {index + 1}
            </span>
            <span className="w-full truncate text-center text-sm font-medium leading-tight">
              {pad.label}
            </span>
            {pad.sublabel ? (
              <span className="w-full truncate text-center font-mono text-xs leading-tight tabular-nums opacity-80">
                {pad.sublabel}
              </span>
            ) : null}
          </button>
        );
        const menu = renderMenu?.(index);
        if (menu === null || menu === undefined || menu === false) {
          return button;
        }
        return (
          <ContextMenu key={index}>
            <ContextMenuTrigger asChild>{button}</ContextMenuTrigger>
            <ContextMenuContent>{menu}</ContextMenuContent>
          </ContextMenu>
        );
      })}
    </div>
  );
}
