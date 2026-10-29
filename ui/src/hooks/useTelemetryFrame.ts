import { useEffect, useRef } from "react";
import type { EngineTelemetry } from "../ipc/bindings";
import { telemetryStore } from "../state/telemetry";

/**
 * Calls `apply` once per animation frame with the newest telemetry, but only
 * when a new snapshot arrived since the last call. `apply` should write to
 * DOM refs; it must not set React state, so no render happens per tick.
 */
export function useTelemetryFrame(apply: (telemetry: EngineTelemetry | null) => void): void {
  const applyRef = useRef(apply);
  useEffect(() => {
    applyRef.current = apply;
  });

  useEffect(() => {
    let frame = 0;
    let lastSequence = -1;
    let first = true;
    const tick = () => {
      const { telemetry } = telemetryStore.getState();
      const sequence = telemetry?.sequence ?? -1;
      if (first || sequence !== lastSequence) {
        first = false;
        lastSequence = sequence;
        applyRef.current(telemetry);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);
}
