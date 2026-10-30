import { useRef } from "react";
import { useTelemetryFrame } from "../hooks/useTelemetryFrame";
import { formatRemaining, formatTime } from "../lib/format";
import { framesToSeconds } from "../state/telemetry";
import { DeckOverview } from "../viz/DeckWaveforms";

interface ProgressBarProps {
  deck: number;
  label: string;
}

/**
 * Elapsed and remaining time plus the seekable overview waveform. Times are
 * written to the DOM from the telemetry store once per animation frame; React
 * never re-renders for a telemetry tick.
 */
export function ProgressBar({ deck, label }: ProgressBarProps) {
  const elapsedRef = useRef<HTMLSpanElement>(null);
  const remainingRef = useRef<HTMLSpanElement>(null);

  useTelemetryFrame((telemetry) => {
    const d = telemetry?.decks[deck];
    const rate = telemetry?.sample_rate ?? 0;
    const position = d ? framesToSeconds(d.position, rate) : 0;
    const duration = d ? framesToSeconds(d.duration, rate) : 0;
    if (elapsedRef.current) {
      elapsedRef.current.textContent = formatTime(position);
    }
    if (remainingRef.current) {
      remainingRef.current.textContent = formatRemaining(Math.max(0, duration - position));
    }
  });

  return (
    <div className="flex flex-col gap-1">
      <DeckOverview deck={deck} label={label} />
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
