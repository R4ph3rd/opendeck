import { useRef, type RefObject } from "react";
import { useTelemetryFrame } from "../hooks/useTelemetryFrame";
import type { EngineTelemetry, StereoMeter } from "../ipc/bindings";
import { isClipping, meterFraction } from "../state/telemetry";

/** Where a meter reads from: a deck channel or the master bus. */
export type MeterSource = { deck: number } | "master";

interface LevelMeterProps {
  source: MeterSource;
  label: string;
}

/** Green to amber to red, from the meter tokens. */
const GRADIENT =
  "linear-gradient(to top, var(--meter-green) 0%, var(--meter-green) 60%, var(--meter-amber) 80%, var(--meter-red) 100%)";

const EMPTY_CLIP = "inset(100% 0 0 0)";

function readMeter(telemetry: EngineTelemetry | null, source: MeterSource): StereoMeter | null {
  if (!telemetry) {
    return null;
  }
  return source === "master" ? telemetry.master : (telemetry.decks[source.deck]?.meter ?? null);
}

/**
 * Two vertical peak bars (left, right). Written to the DOM once per
 * animation frame from the telemetry store; React does not render per tick.
 * ui-viz replaces this with a polished meter in M4.
 */
export function LevelMeter({ source, label }: LevelMeterProps) {
  const root = useRef<HTMLDivElement>(null);
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);
  const clipLeft = useRef<HTMLDivElement>(null);
  const clipRight = useRef<HTMLDivElement>(null);

  useTelemetryFrame((telemetry) => {
    const meter = readMeter(telemetry, source);
    const l = meterFraction(meter?.peak_l ?? null);
    const r = meterFraction(meter?.peak_r ?? null);
    if (left.current) left.current.style.clipPath = `inset(${(1 - l) * 100}% 0 0 0)`;
    if (right.current) right.current.style.clipPath = `inset(${(1 - r) * 100}% 0 0 0)`;
    if (clipLeft.current) clipLeft.current.hidden = !isClipping(meter?.peak_l ?? null);
    if (clipRight.current) clipRight.current.hidden = !isClipping(meter?.peak_r ?? null);
    root.current?.setAttribute("aria-valuenow", String(Math.max(l, r).toFixed(2)));
  });

  const bar = (fill: RefObject<HTMLDivElement | null>, clip: RefObject<HTMLDivElement | null>) => (
    <div className="relative h-full w-2 overflow-hidden rounded-sm bg-meter-off">
      <div
        ref={fill}
        className="absolute inset-0"
        style={{ backgroundImage: GRADIENT, clipPath: EMPTY_CLIP }}
      />
      <div ref={clip} hidden className="absolute inset-x-0 top-0 h-1 bg-meter-clip" />
    </div>
  );

  return (
    <div
      ref={root}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={0}
      className="flex h-full gap-px"
    >
      {bar(left, clipLeft)}
      {bar(right, clipRight)}
    </div>
  );
}
