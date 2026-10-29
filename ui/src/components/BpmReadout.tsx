import { useRef } from "react";
import { useTelemetryFrame } from "../hooks/useTelemetryFrame";

/** Shown until a BPM is known. Analysis lands in M3; manual entry comes later. */
export const BPM_PLACEHOLDER = "--";

/** Effective BPM of a deck, written to the DOM per frame. */
export function BpmReadout({ deck }: { deck: number }) {
  const valueRef = useRef<HTMLOutputElement>(null);

  useTelemetryFrame((telemetry) => {
    const bpm = telemetry?.decks[deck]?.bpm ?? null;
    if (valueRef.current) {
      valueRef.current.textContent = bpm !== null && bpm > 0 ? bpm.toFixed(1) : BPM_PLACEHOLDER;
    }
  });

  return (
    <div className="flex items-baseline gap-1">
      <output
        ref={valueRef}
        aria-label={`Deck ${deck + 1} BPM`}
        className="font-mono text-xl tabular-nums text-fg"
        data-testid={`bpm-${deck}`}
      >
        {BPM_PLACEHOLDER}
      </output>
      <span className="text-xs text-fg-muted">BPM</span>
    </div>
  );
}
