import { useMemo } from "react";
import type { EngineTelemetry, StereoMeter } from "../ipc/bindings";
import { telemetryStore } from "../state/telemetry";
import { LevelMeterCanvas } from "../viz/LevelMeter";

/** Where a meter reads from: a deck channel, the master bus or the sampler bus. */
export type MeterSource = { deck: number } | "master" | "sampler";

interface LevelMeterProps {
  source: MeterSource;
  label: string;
}

function readMeter(telemetry: EngineTelemetry | null, source: MeterSource): StereoMeter | null {
  if (!telemetry) {
    return null;
  }
  if (source === "master") {
    return telemetry.master;
  }
  if (source === "sampler") {
    return telemetry.sampler_meter;
  }
  return telemetry.decks[source.deck]?.meter ?? null;
}

/** Stereo peak and RMS meter for a deck channel, the master or the sampler bus, drawn from the telemetry store. */
export function LevelMeter({ source, label }: LevelMeterProps) {
  const key = typeof source === "string" ? source : source.deck;
  const read = useMemo(
    () => () =>
      readMeter(telemetryStore.getState().telemetry, typeof key === "string" ? key : { deck: key }),
    [key],
  );
  return <LevelMeterCanvas label={label} read={read} />;
}
