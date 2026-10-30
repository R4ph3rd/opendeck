import type { Meta, StoryObj } from "@storybook/react-vite";
import { useEffect, useRef, useState } from "react";
import { NO_MARKERS } from "./markers";
import { Overview } from "./Overview";
import { storyDeck, storyTrack } from "./storyData";
import { Waveform } from "./Waveform";

const meta = {
  title: "Viz/Perf",
  parameters: {
    docs: {
      description: {
        component:
          "Two decks, each a zoomed waveform and an overview, on a 10 minute synthetic track. " +
          "The HUD samples requestAnimationFrame intervals: mean, p95, max, and frames over 20 ms. " +
          "Results are also written to `window.__vizPerf` for headless runs. Frame intervals are capped by the display refresh rate, so read p95 and the late-frame count, not the mean.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

interface PerfResult {
  frames: number;
  meanMs: number;
  p95Ms: number;
  maxMs: number;
  late: number;
}

/** Samples frame intervals for a rolling window and publishes them. */
function useFrameStats(): PerfResult | null {
  const [stats, setStats] = useState<PerfResult | null>(null);
  const times = useRef<number[]>([]);
  useEffect(() => {
    let raf = 0;
    let last = performance.now();
    let report = last;
    const tick = (now: number) => {
      times.current.push(now - last);
      if (times.current.length > 600) times.current.shift();
      last = now;
      if (now - report > 500) {
        report = now;
        const sorted = [...times.current].sort((a, b) => a - b);
        const sum = sorted.reduce((a, b) => a + b, 0);
        const result: PerfResult = {
          frames: sorted.length,
          meanMs: sum / sorted.length,
          p95Ms: sorted[Math.floor(sorted.length * 0.95)] ?? 0,
          maxMs: sorted[sorted.length - 1] ?? 0,
          late: sorted.filter((v) => v > 20).length,
        };
        (window as unknown as { __vizPerf?: PerfResult }).__vizPerf = result;
        setStats(result);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);
  return stats;
}

function TwoDecks() {
  const seconds = 600;
  const track = storyTrack(seconds);
  const [a] = useState(() => storyDeck(seconds, { position: 60, rate: 1.04 }));
  const [b] = useState(() => storyDeck(seconds, { position: 180, rate: 0.96 }));
  const [zoomA, setZoomA] = useState(2);
  const [zoomB, setZoomB] = useState(3);
  const stats = useFrameStats();
  return (
    <div className="flex flex-col gap-2 p-2">
      <p className="font-mono text-xs text-fg-muted" data-testid="perf-hud">
        {stats
          ? `${stats.frames} frames, mean ${stats.meanMs.toFixed(2)} ms, p95 ${stats.p95Ms.toFixed(2)} ms, max ${stats.maxMs.toFixed(1)} ms, late (>20 ms) ${stats.late}`
          : "Sampling..."}
      </p>
      {[
        { deck: a, zoom: zoomA, setZoom: setZoomA, name: "A" },
        { deck: b, zoom: zoomB, setZoom: setZoomB, name: "B" },
      ].map(({ deck, zoom, setZoom, name }) => (
        <div key={name} className="flex flex-col gap-1">
          <Waveform
            label={`Deck ${name} waveform`}
            levels={track.peaks.detail}
            markers={track.markers}
            frameSource={deck.source}
            zoomIndex={zoom}
            onZoomChange={setZoom}
            onScrub={(s) => deck.seek(s)}
            className="h-28"
          />
          <Overview
            label={`Deck ${name} overview`}
            level={track.peaks.overview}
            markers={name === "A" ? track.markers : NO_MARKERS}
            frameSource={deck.source}
            onSeek={(s) => deck.seek(s)}
          />
        </div>
      ))}
    </div>
  );
}

export const TenMinuteTwoDecks: Story = { render: () => <TwoDecks /> };
