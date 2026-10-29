import type { Meta, StoryObj } from "@storybook/react-vite";
import { Separator } from "./separator";

const meta = { title: "Components/Separator", component: Separator } satisfies Meta<typeof Separator>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Horizontal: Story = { render: () => <div className="w-64 text-sm">Above<Separator className="my-2" />Below</div> };
export const Vertical: Story = {
  render: () => (
    <div className="flex h-6 items-center gap-2 text-sm">
      A<Separator orientation="vertical" />B
    </div>
  ),
};
