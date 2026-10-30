import type { Meta, StoryObj } from "@storybook/react-vite";
import { SegmentedControl } from "./segmented-control";

const meta = {
  title: "Components/SegmentedControl",
  component: SegmentedControl,
  args: {
    "aria-label": "Tempo range",
    defaultValue: "8",
    options: [
      { value: "6", label: "6%" },
      { value: "8", label: "8%" },
      { value: "16", label: "16%" },
      { value: "50", label: "50%" },
    ],
  },
  parameters: {
    docs: {
      description: {
        component:
          "Choose exactly one of a few values: tempo range, beat size, quantize. A radio group: arrows move and select, Tab leaves. Always set `aria-label` (or `aria-labelledby`). For independent on/off buttons use ToggleGroup instead. Icon-only options need their own `aria-label`.",
      },
    },
  },
} satisfies Meta<typeof SegmentedControl>;
export default meta;
type Story = StoryObj<typeof meta>;

export const TempoRange: Story = {};
export const BeatSize: Story = {
  args: {
    "aria-label": "Loop size",
    defaultValue: "4",
    size: "sm",
    options: ["1/2", "1", "2", "4", "8", "16"].map((v) => ({ value: v, label: v })),
  },
};
export const WithDisabledOption: Story = {
  args: {
    options: [
      { value: "8", label: "8%" },
      { value: "16", label: "16%", disabled: true },
    ],
  },
};
