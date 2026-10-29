import type { Meta, StoryObj } from "@storybook/react-vite";
import { Badge } from "./badge";

const meta = { title: "Components/Badge", component: Badge, args: { children: "128 BPM" } } satisfies Meta<typeof Badge>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
export const Variants: Story = {
  render: () => (
    <div className="flex gap-2">
      <Badge>Neutral</Badge>
      <Badge variant="accent">Accent</Badge>
      <Badge variant="success">Synced</Badge>
      <Badge variant="warning">Loop</Badge>
      <Badge variant="danger">Clip</Badge>
    </div>
  ),
};
