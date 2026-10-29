import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { useTelemetryFrame } from "../hooks/useTelemetryFrame";
import { formatRemaining, formatTime } from "../lib/format";
import { seekBy, seekFraction } from "../state/actions";
import { framesToSeconds } from "../state/telemetry";

/** Arrow keys on the focused bar seek this far. */
const SEEK_STEP_SECONDS = 5;

interface ProgressBarProps {
  deck: number;
  label: string;
}

/**
 * Elapsed and remaining time plus a seekable bar. The playhead is written to
 * the DOM from the telemetry store once per animation frame; React never
 * re-renders for a telemetry tick. Waveforms replace the bar in milestone 4.
 */
export function ProgressBar({ deck, label }: ProgressBarProps) {
  const barRef = useRef<HTMLDivElement>(null);
  const fillRef = useRef<HTMLDivElement>(null);
  const elapsedRef = useRef<HTMLSpanElement>(null);
  const remainingRef = useRef<HTMLSpanElement>(null);

  useTelemetryFrame((telemetry) => {
    const d = telemetry?.decks[deck];
    const rate = telemetry?.sample_rate ?? 0;
    const position = d ? framesToSeconds(d.position, rate) : 0;
    const duration = d ? framesToSeconds(d.duration, rate) : 0;
    const fraction = duration > 0 ? Math.min(1, position / duration) : 0;
    if (fillRef.current) {
      fillRef.current.style.transform = `scaleX(${fraction})`;
    }
    if (elapsedRef.current) {
      elapsedRef.current.textContent = formatTime(position);
    }
    if (remainingRef.current) {
      remainingRef.current.textContent = formatRemaining(Math.max(0, duration - position));
    }
    if (barRef.current) {
      barRef.current.setAttribute("aria-valuenow", String(Math.floor(position)));
      barRef.current.setAttribute("aria-valuemax", String(Math.floor(duration)));
    }
  });

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (rect.width > 0) {
      seekFraction(deck, (event.clientX - rect.left) / rect.width);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "ArrowRight") {
      event.preventDefault();
      seekBy(deck, SEEK_STEP_SECONDS);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      seekBy(deck, -SEEK_STEP_SECONDS);
    }
  };

  return (
    <div className="flex flex-col gap-1">
      <div
        ref={barRef}
        role="slider"
        tabIndex={0}
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={0}
        aria-valuenow={0}
        onPointerDown={onPointerDown}
        onKeyDown={onKeyDown}
        className="relative h-3 cursor-pointer overflow-hidden rounded-sm bg-surface-hover focus-visible:outline-2 focus-visible:outline-focus-ring"
      >
        <div
          ref={fillRef}
          className="absolute inset-0 origin-left bg-deck-a opacity-[var(--wave-played-opacity)]"
          style={{ transform: "scaleX(0)" }}
        />
      </div>
      <div className="flex justify-between font-mono text-sm tabular-nums text-fg-muted">
        <span ref={elapsedRef} data-testid={`elapsed-${deck}`}>
          0:00
        </span>
        <span ref={remainingRef} data-testid={`remaining-${deck}`}>
          -0:00
        </span>
      </div>
    </div>
  );
}
