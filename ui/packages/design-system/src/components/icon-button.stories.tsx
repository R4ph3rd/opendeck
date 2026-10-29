import type { Meta, StoryObj } from "@storybook/react-vite";
import { Play, Repeat, Settings } from "lucide-react";
import { IconButton } from "./icon-button";

const meta = {
  title: "Components/IconButton",
  component: IconButton,
  args: { "aria-label": "Settings" },
} satisfies Meta<typeof IconButton>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { render: (args) => <IconButton {...args}><Settings className="size-4" /></IconButton> };
export const Sizes: Story = {
  render: () => (
    <div className="flex items-center gap-2">
      <IconButton size="sm" aria-label="Play"><Play className="size-3.5" /></IconButton>
      <IconButton size="md" aria-label="Play"><Play className="size-4" /></IconButton>
      <IconButton size="lg" aria-label="Play"><Play className="size-4" /></IconButton>
    </div>
  ),
};
export const Pressed: Story = {
  render: () => (
    <IconButton aria-label="Loop" aria-pressed>
      <Repeat className="size-4" />
    </IconButton>
  ),
};
