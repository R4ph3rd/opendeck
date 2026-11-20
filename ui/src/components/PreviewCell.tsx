import { cn } from "@opendeck/design-system";
import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import type { TrackRowDto } from "../ipc/bindings";
import { useTelemetryFrame } from "../hooks/useTelemetryFrame";
import { MINI_COLUMNS, miniPeaks } from "../lib/miniPeaks";
import { displayTitle } from "../lib/library";
import { togglePreview, usePreviewStore } from "../state/preview";

/**
 * Wait in ms before a row asks for its peaks. A row that scrolls past in
 * under this time never asks, so a fast scroll costs nothing.
 */
export const PEAK_FETCH_DELAY_MS = 100;

/** Rows with stored analysis. A stale one was analysed by an older analyzer; its peaks still draw. */
const hasAnalysis = (track: TrackRowDto) =>
  track.analysis_status === "done" || track.analysis_status === "stale";

/** The mini waveform of a track, fetched once the row is mounted and settled. */
function useMiniBars(track: number, enabled: boolean): Uint8Array | null {
  const [fetched, setFetched] = useState<{ id: number; bars: Uint8Array } | null>(null);
  useEffect(() => {
    if (!enabled || miniPeaks.peek(track)) {
      return undefined;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      void miniPeaks.get(track).then((bars) => {
        if (!cancelled && bars) {
          setFetched({ id: track, bars });
        }
      });
    }, PEAK_FETCH_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [track, enabled]);
  const cached = enabled ? miniPeaks.peek(track) : null;
  return cached ?? (enabled && fetched?.id === track ? fetched.bars : null);
}

/** Vertical bars mirrored around the middle of a `MINI_COLUMNS` by 100 box. */
export function barsPath(bars: Uint8Array): string {
  let d = "";
  bars.forEach((value, i) => {
    const half = Math.max(2, (value / 255) * 48);
    d += `M${i + 0.5} ${50 - half}V${50 + half}`;
  });
  return d;
}

/** Elapsed shade and playhead of the track being previewed. Writes to the DOM per frame, never to React state. */
function Playhead({ token }: { token: number | null }) {
  const shade = useRef<HTMLSpanElement>(null);
  const head = useRef<HTMLSpanElement>(null);
  useTelemetryFrame((telemetry) => {
    const preview = telemetry?.preview;
    const known = token !== null && preview?.token === token && preview.duration > 0;
    const fraction = known ? Math.min(1, Math.max(0, preview.position / preview.duration)) : 0;
    if (shade.current) {
      shade.current.style.width = `${fraction * 100}%`;
    }
    if (head.current) {
      head.current.style.left = `${fraction * 100}%`;
      head.current.style.visibility = known ? "visible" : "hidden";
    }
  });
  return (
    <>
      <span
        ref={shade}
        aria-hidden
        className="pointer-events-none absolute inset-y-0 left-0 bg-wave-bg/55"
      />
      <span
        ref={head}
        aria-hidden
        className="pointer-events-none absolute inset-y-0 w-px bg-playhead"
        style={{ visibility: "hidden" }}
      />
    </>
  );
}

/** A cell that must not select the row, load the track or start a drag. */
const swallow = (event: { stopPropagation: () => void }) => event.stopPropagation();

/**
 * Preview column: a muted overview waveform once the track is analysed, an
 * empty slot until then. A click starts the preview at that point of the
 * track and shows a playhead; a click on the previewed track stops it, and a
 * click on another track switches.
 */
export function PreviewCell({ track }: { track: TrackRowDto }) {
  const analysed = hasAnalysis(track);
  const bars = useMiniBars(track.id, analysed);
  const path = useMemo(() => (bars ? barsPath(bars) : ""), [bars]);
  const active = usePreviewStore((s) => s.trackId === track.id);
  const token = usePreviewStore((s) => (s.trackId === track.id ? s.token : null));

  if (!analysed || !bars) {
    const reason = analysed
      ? "Loading the waveform"
      : track.analysis_status === "failed"
        ? "Analysis failed, so there is no preview"
        : "Preview is available once the track is analysed";
    return (
      <span
        role="img"
        aria-label={`No preview for ${displayTitle(track)}`}
        title={reason}
        className="flex h-4 w-full items-center"
        onPointerDown={swallow}
      >
        <span aria-hidden className="h-px w-full bg-border" />
      </span>
    );
  }

  const onClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    const rect = event.currentTarget.getBoundingClientRect();
    // A keyboard click has no pointer position: start at the beginning.
    const fraction =
      event.detail === 0 || rect.width <= 0 ? 0 : (event.clientX - rect.left) / rect.width;
    togglePreview(track.id, Math.min(1, Math.max(0, fraction)));
  };

  return (
    <button
      type="button"
      aria-label={`Preview ${displayTitle(track)}`}
      aria-pressed={active}
      aria-busy={active && token === null}
      title={active ? "Click to stop the preview" : "Click to preview from here"}
      onClick={onClick}
      onDoubleClick={swallow}
      onPointerDown={swallow}
      className={cn(
        "relative block h-4 w-full cursor-pointer overflow-hidden rounded-sm bg-wave-bg",
        "focus-visible:outline-2 focus-visible:outline-focus-ring",
        active && token === null && "animate-pulse",
      )}
    >
      <svg
        viewBox={`0 0 ${MINI_COLUMNS} 100`}
        preserveAspectRatio="none"
        aria-hidden
        className="size-full"
      >
        <path
          d={path}
          className="stroke-wave-low opacity-70"
          strokeWidth={0.8}
          shapeRendering="crispEdges"
        />
      </svg>
      {active ? <Playhead token={token} /> : null}
    </button>
  );
}
