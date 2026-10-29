import type { Meta, StoryObj } from "@storybook/react-vite";
import { Slider } from "./slider";

const meta = {
  title: "Components/Slider",
  component: Slider,
  args: { defaultValue: [50], max: 100, step: 1, "aria-label": "Volume" },
} satisfies Meta<typeof Slider>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Horizontal: Story = { render: (args) => <div className="w-64"><Slider {...args} /></div> };
export const Vertical: Story = {
  render: (args) => (
    <div className="h-40">
      <Slider {...args} orientation="vertical" />
    </div>
  ),
};
export const Range: Story = {
  render: (args) => <div className="w-64"><Slider {...args} defaultValue={[20, 70]} aria-label="Loop range" /></div>,
};
export const ChannelFader: Story = {
  args: { orientation: "vertical", "aria-label": "Channel gain", defaultValue: [80] },
  render: (args) => <Slider {...args} trackLength="12rem" />,
};
export const TempoFaderCenterDetent: Story = {
  args: {
    orientation: "vertical",
    "aria-label": "Tempo",
    min: -8,
    max: 8,
    step: 0.1,
    defaultValue: [0],
    detent: "center",
    "aria-valuetext": "0 percent",
  },
  render: (args) => <Slider {...args} trackLength="16rem" />,
};
export const HorizontalDetent: Story = {
  args: { detent: "center", "aria-label": "Crossfader", defaultValue: [50] },
  render: (args) => <div className="w-64"><Slider {...args} /></div>,
};
