import { useMemo } from "react";
import type { EngineTelemetry, StereoMeter } from "../ipc/bindings";
import { telemetryStore } from "../state/telemetry";
import { LevelMeterCanvas } from "../viz/LevelMeter";

/** Where a meter reads from: a deck channel or the master bus. */
export type MeterSource = { deck: number } | "master";

interface LevelMeterProps {
  source: MeterSource;
  label: string;
}

function readMeter(telemetry: EngineTelemetry | null, source: MeterSource): StereoMeter | null {
  if (!telemetry) {
    return null;
  }
  return source === "master" ? telemetry.master : (telemetry.decks[source.deck]?.meter ?? null);
}

/** Stereo peak and RMS meter for a deck channel or the master bus, drawn from the telemetry store. */
export function LevelMeter({ source, label }: LevelMeterProps) {
  const deck = source === "master" ? null : source.deck;
  const read = useMemo(
    () => () => readMeter(telemetryStore.getState().telemetry, deck === null ? "master" : { deck }),
    [deck],
  );
  return <LevelMeterCanvas label={label} read={read} />;
}
