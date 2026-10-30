import type { Meta, StoryObj } from "@storybook/react-vite";
import { Skeleton } from "./skeleton";

const meta = {
  title: "Components/Skeleton",
  component: Skeleton,
  parameters: {
    docs: {
      description: {
        component:
          "Placeholder shaped like the content that is loading. Size with classes. It is hidden from assistive tech, so set `aria-busy` on the container and replace it when data arrives. The pulse stops under reduced motion.",
      },
    },
  },
} satisfies Meta<typeof Skeleton>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Line: Story = { args: { className: "h-4 w-48" } };
export const TrackRows: Story = {
  render: () => (
    <div aria-busy="true" className="flex w-80 flex-col gap-2">
      {Array.from({ length: 4 }, (_, i) => (
        <div key={i} className="flex items-center gap-2">
          <Skeleton className="size-8" />
          <div className="flex flex-1 flex-col gap-1">
            <Skeleton className="h-3 w-3/4" />
            <Skeleton className="h-3 w-1/2" />
          </div>
        </div>
      ))}
    </div>
  ),
};
