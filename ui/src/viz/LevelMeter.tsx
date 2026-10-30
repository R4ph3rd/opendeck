import { cn } from "@opendeck/design-system";
import { useMemo, useRef } from "react";
import type { StereoMeter } from "../ipc/bindings";
import { isClipping, meterFraction } from "../state/telemetry";
import type { TokenName, VizColors } from "./colors";
import { prefersReducedMotion } from "./colors";
import { AMBER_FROM, HOLD_MS, RED_FROM, newHold, updateClip, updateHold, type HoldState } from "./meterMath";
import { useCanvasLoop } from "./useCanvasLoop";

/** Reads the current meter values. Called once per animation frame. */
export type MeterReader = () => StereoMeter | null;

export interface LevelMeterCanvasProps {
  label: string;
  read: MeterReader;
  className?: string;
}

interface ChannelState {
  hold: HoldState;
  clip: { t: number };
}

const newChannel = (): ChannelState => ({ hold: newHold(), clip: { t: 0 } });

/** Fills `[0, fraction]` of the column height with the green, amber and red zones. */
function fillZones(
  ctx: CanvasRenderingContext2D,
  colors: VizColors,
  x: number,
  w: number,
  h: number,
  fraction: number,
): void {
  const zones: readonly [number, number, TokenName][] = [
    [0, AMBER_FROM, "meter-green"],
    [AMBER_FROM, RED_FROM, "meter-amber"],
    [RED_FROM, 1, "meter-red"],
  ];
  for (const [from, to, token] of zones) {
    const top = Math.min(fraction, to);
    if (top > from) {
      ctx.fillStyle = colors.color[token];
      const y0 = h * (1 - top);
      ctx.fillRect(x, y0, w, h * (top - from));
    }
  }
}

/**
 * Stereo level meter on a canvas. Per channel: RMS as the wide bar, peak as
 * the narrow bar inside it, a peak-hold tick and a latching clip lamp. Zone
 * colours come from the meter tokens: green, amber from -12 dBFS, red from
 * -3 dBFS. The scale is `METER_FLOOR_DB` to 0 dBFS.
 */
export function LevelMeterCanvas({ label, read, className }: LevelMeterCanvasProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const channelsRef = useRef<ChannelState[] | null>(null);
  const shown = useRef("");
  const reduced = useMemo(() => prefersReducedMotion(), []);

  const canvasRef = useCanvasLoop((f) => {
    const { ctx, width, height, dpr, colors, now } = f;
    const channels = (channelsRef.current ??= [newChannel(), newChannel()]);
    const meter = read();
    const peaks = [meter?.peak_l ?? null, meter?.peak_r ?? null] as const;
    const rms = [meter?.rms_l ?? null, meter?.rms_r ?? null] as const;
    const peakF = [meterFraction(peaks[0]), meterFraction(peaks[1])] as const;
    const rmsF = [meterFraction(rms[0]), meterFraction(rms[1])] as const;
    const gap = Math.max(1, Math.round(dpr));
    const colW = Math.floor((width - gap) / 2);
    const lampH = Math.max(3, Math.round(3 * dpr));
    const tick = Math.max(1, Math.round(dpr));

    const holds = channels.map((ch, i) => {
      const level = peakF[i] ?? 0;
      if (reduced && now - ch.hold.at > HOLD_MS) {
        ch.hold.value = level;
      }
      return updateHold(ch.hold, level, now);
    });
    const lamps = channels.map((ch, i) => updateClip(ch.clip, isClipping(peaks[i] ?? null), now));

    // Redraw only when something visible changed.
    const key = [
      peakF[0].toFixed(3),
      peakF[1].toFixed(3),
      rmsF[0].toFixed(3),
      rmsF[1].toFixed(3),
      holds[0]?.toFixed(3),
      holds[1]?.toFixed(3),
      lamps.join(),
      width,
      height,
    ].join("|");
    const root = rootRef.current;
    if (root) {
      const value = Math.max(peakF[0], peakF[1]).toFixed(2);
      if (root.getAttribute("aria-valuenow") !== value) {
        root.setAttribute("aria-valuenow", value);
      }
    }
    if (!f.invalidated && key === shown.current) return;
    shown.current = key;

    ctx.clearRect(0, 0, width, height);
    for (let i = 0; i < 2; i += 1) {
      const x = i * (colW + gap);
      ctx.fillStyle = colors.color["meter-off"];
      ctx.fillRect(x, 0, colW, height);
      // RMS: wide and dimmer.
      ctx.globalAlpha = 0.55;
      fillZones(ctx, colors, x, colW, height, rmsF[i] ?? 0);
      // Peak: narrow and full strength, centred.
      ctx.globalAlpha = 1;
      const inner = Math.max(2, Math.floor(colW / 2));
      fillZones(ctx, colors, x + Math.floor((colW - inner) / 2), inner, height, peakF[i] ?? 0);
      // Hold tick.
      const hold = holds[i] ?? 0;
      if (hold > 0.001) {
        const token: TokenName = hold >= RED_FROM ? "meter-red" : hold >= AMBER_FROM ? "meter-amber" : "fg";
        ctx.fillStyle = colors.color[token];
        ctx.fillRect(x, Math.min(height - tick, height * (1 - hold)), colW, tick);
      }
      if (lamps[i]) {
        ctx.fillStyle = colors.color["meter-clip"];
        ctx.fillRect(x, 0, colW, lampH);
      }
    }
  });

  return (
    <div
      ref={rootRef}
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={1}
      aria-valuenow={0}
      className={cn("h-full w-4", className)}
    >
      <canvas ref={canvasRef} className="block size-full" aria-hidden />
    </div>
  );
}
