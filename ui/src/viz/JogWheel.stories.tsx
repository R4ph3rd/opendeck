import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { JogWheel } from "./JogWheel";
import { storyDeck } from "./storyData";

const meta = {
  title: "Viz/JogWheel",
  component: JogWheel,
  parameters: {
    docs: {
      description: {
        component:
          "Jog wheel on a canvas. The platter turns with the playhead at 33 1/3 rpm. Press on the platter to touch it (`jog_touch`) " +
          "and drag around the centre to send rotation deltas (`jog_turn`, summed per animation frame). " +
          "The outer ring nudges without touching. Shift while dragging does a fast search (`jog_search`). " +
          "The cue marker on the ring meets the position marker when the playhead is on the cue. " +
          "Arrow keys on the focused wheel nudge. The log below shows what would be sent.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

function Demo({ playing = true, disabled = false }: { playing?: boolean; disabled?: boolean }) {
  const [deck] = useState(() => storyDeck(240, { playing, position: 12 }));
  const [log, setLog] = useState<string[]>([]);
  const push = (line: string) => setLog((l) => [line, ...l].slice(0, 6));
  return (
    <div className="flex items-start gap-6 p-4">
      <JogWheel
        deck={0}
        label="Deck A jog wheel"
        frameSource={deck.source}
        cuePoint={14}
        disabled={disabled}
        className="size-40"
        onTouch={(t) => {
          deck.setPlaying(!t && playing);
          push(`jog_touch ${t}`);
        }}
        onTurn={(r) => {
          deck.seek(deck.source(performance.now()).position + r * 1.8);
          push(`jog_turn ${r.toFixed(4)}`);
        }}
        onSearch={(b) => push(`jog_search ${b.toFixed(2)} beats`)}
      />
      <pre className="min-h-24 font-mono text-xs text-fg-muted">{log.join("\n") || "No events yet"}</pre>
    </div>
  );
}

export const Playing: Story = { render: () => <Demo /> };
export const Paused: Story = { render: () => <Demo playing={false} /> };
export const Disabled: Story = { render: () => <Demo disabled /> };
