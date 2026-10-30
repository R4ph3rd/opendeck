import type { Meta, StoryObj } from "@storybook/react-vite";
import { Callout } from "./callout";

const meta = {
  title: "Components/Callout",
  component: Callout,
  decorators: [(Story) => <div className="w-96"><Story /></div>],
  args: { title: "Heads up", children: "Changes apply the next time you load a track." },
  parameters: {
    docs: {
      description: {
        component:
          "Inline message inside a page or dialog. `info` and `success` and `warning` announce politely (`role=status`); `danger` uses `role=alert`. The icon and title carry the meaning in addition to color. Use Toast for transient feedback.",
      },
    },
  },
} satisfies Meta<typeof Callout>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Info: Story = { args: { variant: "info" } };
export const Success: Story = { args: { variant: "success", title: "Library updated" } };
export const Warning: Story = {
  args: { variant: "warning", title: "Audio device changed", children: "Playback was paused." },
};
export const Danger: Story = {
  args: { variant: "danger", title: "Import failed", children: "3 files could not be read." },
};
export const NoTitle: Story = { args: { title: undefined } };
