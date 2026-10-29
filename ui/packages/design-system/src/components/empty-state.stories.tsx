import type { Meta, StoryObj } from "@storybook/react-vite";
import { Disc3 } from "lucide-react";
import { Button } from "./button";
import { EmptyState } from "./empty-state";

const meta = { title: "Components/EmptyState", component: EmptyState } satisfies Meta<typeof EmptyState>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    icon: <Disc3 />,
    title: "No tracks yet",
    description: "Add a folder to build your collection.",
    action: <Button variant="primary">Add folder</Button>,
    className: "h-72 rounded-lg border border-border bg-surface",
  },
};
