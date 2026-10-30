import type { Meta, StoryObj } from "@storybook/react-vite";
import { StatusDot } from "./status-dot";

const meta = {
  title: "Components/StatusDot",
  component: StatusDot,
  args: { label: "SYNC", tone: "sync", state: "on" },
  parameters: {
    docs: {
      description: {
        component: [
          "LED indicator for SYNC, MASTER, loop and connection state.",
          "",
          "- `state`: `on` (filled), `off` (hollow ring), `blink` (steps at `--duration-blink`; holds steady under reduced motion).",
          "- `tone` maps to tokens: `sync`, `master`, `loop`, `accent`, `success`, `warning`, `danger`, `info`.",
          "- State is announced as text (\"SYNC, on\"). Keep the visible label where space allows; use `hideLabel` for icon-dense strips.",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof StatusDot>;
export default meta;
type Story = StoryObj<typeof meta>;

export const On: Story = {};
export const Off: Story = { args: { state: "off" } };
export const Blink: Story = { args: { state: "blink", label: "LOOP", tone: "loop" } };
export const Tones: Story = {
  render: () => (
    <div className="flex flex-wrap gap-4">
      <StatusDot label="SYNC" tone="sync" />
      <StatusDot label="MASTER" tone="master" />
      <StatusDot label="LOOP" tone="loop" state="blink" />
      <StatusDot label="REC" tone="danger" state="blink" />
      <StatusDot label="MIDI" tone="success" />
      <StatusDot label="CLIP" tone="warning" />
      <StatusDot label="LINK" tone="info" state="off" />
      <StatusDot label="Armed" tone="accent" hideLabel />
    </div>
  ),
};
