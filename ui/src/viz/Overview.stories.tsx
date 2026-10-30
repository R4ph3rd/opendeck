import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { NO_MARKERS } from "./markers";
import { Overview } from "./Overview";
import { storyDeck, storyTrack } from "./storyData";

const meta = {
  title: "Viz/Overview",
  component: Overview,
  parameters: {
    docs: {
      description: {
        component:
          "Whole-track overview. The waveform is rendered once into an offscreen canvas per size, peak set and theme; " +
          "each frame blits it and draws the elapsed dimming, loop, cue marks and playhead. Click or drag to seek, arrow keys seek by 5 s.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

function Demo({ seconds = 240, markers = true }: { seconds?: number; markers?: boolean }) {
  const [deck] = useState(() => {
    const d = storyDeck(seconds, { position: seconds * 0.3 });
    d.loop = { start: seconds * 0.55, end: seconds * 0.58 };
    return d;
  });
  const track = storyTrack(seconds);
  return (
    <div className="flex max-w-3xl flex-col gap-2 p-2">
      <Overview
        label="Deck A overview"
        level={track.peaks.overview}
        markers={markers ? track.markers : NO_MARKERS}
        frameSource={deck.source}
        onSeek={(s) => deck.seek(s)}
        className="h-14"
      />
    </div>
  );
}

export const Default: Story = { render: () => <Demo /> };
export const WithoutMarkers: Story = { render: () => <Demo markers={false} /> };
export const TenMinutes: Story = { render: () => <Demo seconds={600} /> };
