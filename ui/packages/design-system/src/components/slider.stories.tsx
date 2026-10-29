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
