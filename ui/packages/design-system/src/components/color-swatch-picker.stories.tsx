import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { ColorSwatchPicker, type CueColor } from "./color-swatch-picker";

const meta = {
  title: "Components/ColorSwatchPicker",
  component: ColorSwatchPicker,
  args: { defaultValue: 1 },
  parameters: {
    docs: {
      description: {
        component:
          "Pick a hot cue color from the 8 `--cue-N` tokens. Store the index (1 to 8), not a hex, so the color follows the theme. A radio group: arrows move and select. The checked swatch shows a check mark and an outer ring, so color is never the only cue. Each swatch is named (for example \"Amber (cue 3)\").",
      },
    },
  },
} satisfies Meta<typeof ColorSwatchPicker>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
export const Subset: Story = { args: { colors: [1, 3, 5, 7] } };
export const Disabled: Story = { args: { disabled: true } };
export const Controlled: Story = {
  render: function Render() {
    const [c, setC] = useState<CueColor>(2);
    return (
      <div className="flex items-center gap-3">
        <ColorSwatchPicker value={c} onValueChange={setC} />
        <span className="font-mono text-xs text-fg-muted">cue-{c}</span>
      </div>
    );
  },
};
