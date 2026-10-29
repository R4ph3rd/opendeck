import type { Meta, StoryObj } from "@storybook/react-vite";
import { Repeat } from "lucide-react";
import { Toggle, ToggleGroup, ToggleGroupItem } from "./toggle";

const meta = { title: "Components/Toggle", component: Toggle } satisfies Meta<typeof Toggle>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Single: Story = {
  render: () => (
    <Toggle aria-label="Loop" variant="outline">
      <Repeat className="size-4" /> Loop
    </Toggle>
  ),
};
export const Group: Story = {
  render: () => (
    <ToggleGroup type="single" defaultValue="4" aria-label="Loop length">
      {["1", "2", "4", "8"].map((n) => (
        <ToggleGroupItem key={n} value={n}>
          {n}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  ),
};
