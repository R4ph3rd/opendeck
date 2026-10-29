import type { Meta, StoryObj } from "@storybook/react-vite";
import { Play } from "lucide-react";
import { IconButton } from "./icon-button";
import { Tooltip } from "./tooltip";

const meta = { title: "Components/Tooltip", component: Tooltip } satisfies Meta<typeof Tooltip>;
export default meta;
type Story = StoryObj<typeof meta>;

export const WithShortcut: Story = {
  args: { content: "Play", shortcut: "Space", children: null },
  render: (args) => (
    <Tooltip {...args}>
      <IconButton aria-label="Play">
        <Play className="size-4" />
      </IconButton>
    </Tooltip>
  ),
};
