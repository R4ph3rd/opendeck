import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { NO_MARKERS } from "./markers";
import { storyDeck, storyTrack } from "./storyData";
import { Waveform } from "./Waveform";

const meta = {
  title: "Viz/Waveform",
  component: Waveform,
  parameters: {
    docs: {
      description: {
        component:
          "Zoomed, centred, scrolling waveform on a 2D canvas, drawn from requestAnimationFrame. " +
          "Low, mid and high bands use `--wave-low/mid/high`; the played half is dimmed with `--wave-played-opacity`. " +
          "Beat grid uses `--beat-grid` and `--beat-grid-downbeat`, hot cues the `--cue-1..8` tokens, loops `--loop-region` and `--loop-edge`. " +
          "The playhead is extrapolated from the last telemetry snapshot (see `interp.ts`). " +
          "Drag to scrub, wheel or +/- to zoom. The data is a deterministic synthetic track.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

function Demo({
  seconds = 180,
  playing = true,
  rate = 1,
  loop = false,
  markers = true,
  zoom = 2,
}: {
  seconds?: number;
  playing?: boolean;
  rate?: number;
  loop?: boolean;
  markers?: boolean;
  zoom?: number;
}) {
  const [deck] = useState(() => {
    const d = storyDeck(seconds, { playing, rate });
    if (loop) d.loop = { start: 3.9, end: 11.6 };
    return d;
  });
  const track = storyTrack(seconds);
  const [zoomIndex, setZoom] = useState(zoom);
  return (
    <div className="flex flex-col gap-2 p-2">
      <Waveform
        label="Deck A waveform"
        levels={track.peaks.detail}
        markers={markers ? track.markers : NO_MARKERS}
        frameSource={deck.source}
        zoomIndex={zoomIndex}
        onZoomChange={setZoom}
        onScrub={(s) => deck.seek(s)}
        className="h-32"
      />
      <p className="text-xs text-fg-muted">Drag to scrub. Wheel, buttons or + and - to zoom.</p>
    </div>
  );
}

export const Playing: Story = { render: () => <Demo /> };
export const Paused: Story = { render: () => <Demo playing={false} /> };
export const FastTempo: Story = { render: () => <Demo rate={1.16} /> };
export const WithLoop: Story = { render: () => <Demo loop /> };
export const ZoomedIn: Story = { render: () => <Demo zoom={0} /> };
export const ZoomedOut: Story = { render: () => <Demo zoom={5} /> };
export const NoOverlays: Story = { render: () => <Demo markers={false} /> };
export const TenMinutes: Story = { render: () => <Demo seconds={600} /> };
