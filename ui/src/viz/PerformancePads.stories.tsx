import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { cueToken } from "./colors";
import { PerformancePads, type PadSpec } from "./PerformancePads";

const meta = {
  title: "Viz/PerformancePads",
  component: PerformancePads,
  parameters: {
    docs: {
      description: {
        component:
          "Generic grid of pads. A lit pad is filled with its colour token, a pressed pad is inset. " +
          "Used for hot cues now; later pad modes (loops, rolls, effects) pass different specs. " +
          "Press and release are reported once each, also when the pointer is lost. Space and Enter work too.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

const hotCues = (set: number[]): PadSpec[] =>
  Array.from({ length: 8 }, (_, i) => ({
    label: String(i + 1),
    ariaLabel: `Hot cue ${i + 1}${set.includes(i) ? "" : " (empty)"}`,
    color: cueToken(i),
    lit: set.includes(i),
  }));

function Demo({ pads, columns = 4, disabled = false }: { pads: PadSpec[]; columns?: 2 | 4 | 8; disabled?: boolean }) {
  const [last, setLast] = useState("Press a pad");
  return (
    <div className="flex max-w-sm flex-col gap-2 p-4">
      <PerformancePads
        label="Pads"
        pads={pads}
        columns={columns}
        disabled={disabled}
        onPress={(i, { shift }) => setLast(`press ${i + 1}${shift ? " (shift)" : ""}`)}
        onRelease={(i) => setLast(`release ${i + 1}`)}
      />
      <p className="font-mono text-xs text-fg-muted">{last}</p>
    </div>
  );
}

export const HotCues: Story = { render: () => <Demo pads={hotCues([0, 1, 2, 4, 7])} /> };
export const AllEmpty: Story = { render: () => <Demo pads={hotCues([])} /> };
export const AllSet: Story = { render: () => <Demo pads={hotCues([0, 1, 2, 3, 4, 5, 6, 7])} /> };
export const SingleRow: Story = { render: () => <Demo pads={hotCues([0, 3, 5])} columns={8} /> };
export const Disabled: Story = { render: () => <Demo pads={hotCues([0, 1])} disabled /> };
export const LoopPads: Story = {
  render: () => (
    <Demo
      pads={["1/8", "1/4", "1/2", "1", "2", "4", "8", "16"].map((label, i) => ({
        label,
        color: "accent",
        lit: i === 3,
      }))}
    />
  ),
};
