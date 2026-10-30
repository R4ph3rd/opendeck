import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { NumberField } from "./number-field";

const meta = {
  title: "Components/NumberField",
  component: NumberField,
  args: { "aria-label": "BPM", defaultValue: 124, min: 20, max: 300, step: 0.1, largeStep: 1, unit: "BPM" },
  parameters: {
    docs: {
      description: {
        component: [
          "Numeric entry for BPM, offsets and gains.",
          "",
          "- Type a value and press Enter or leave the field to commit. Out of range values clamp. Escape discards. A decimal comma is accepted.",
          "- ArrowUp/Down step by `step`; Shift uses `largeStep`; Alt uses a tenth of `step`. PageUp/Down use `largeStep`. Home/End jump to `min`/`max`.",
          "- The mouse wheel steps only while the field has focus, so scrolling a page never changes it.",
          "- Drag the grip on the left sideways to scrub (`scrubPixels` per step, Shift for fine). Keyboard users have the arrows.",
          "- `precision` defaults to the decimals in `step`. `unit` is display only.",
          "- Controlled with `value` + `onValueChange`, or uncontrolled with `defaultValue`.",
        ].join("\n"),
      },
    },
  },
} satisfies Meta<typeof NumberField>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Bpm: Story = {};
export const Integer: Story = { args: { step: 1, largeStep: 10, defaultValue: 8, min: 1, max: 64, unit: "beats" } };
export const Disabled: Story = { args: { disabled: true } };
export const Controlled: Story = {
  render: function Render(args) {
    const [v, setV] = useState(128);
    return (
      <div className="flex items-center gap-3">
        <NumberField {...args} value={v} onValueChange={setV} />
        <span className="font-mono text-xs text-fg-muted">value: {v}</span>
      </div>
    );
  },
};
