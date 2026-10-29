import type { Meta, StoryObj } from "@storybook/react-vite";
import { ScrollArea } from "./scroll-area";

const meta = { title: "Components/ScrollArea", component: ScrollArea } satisfies Meta<typeof ScrollArea>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <ScrollArea className="h-40 w-64 rounded-md border border-border">
      <ul className="p-2 text-sm">
        {Array.from({ length: 30 }, (_, i) => (
          <li key={i} className="py-1 text-fg-muted">
            Track {i + 1}
          </li>
        ))}
      </ul>
    </ScrollArea>
  ),
};
