import type { Meta, StoryObj } from "@storybook/react-vite";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from "./select";

const meta = { title: "Components/Select", component: Select } satisfies Meta<typeof Select>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <div className="w-56">
      <Select defaultValue="master">
        <SelectTrigger aria-label="Output">
          <SelectValue placeholder="Output" />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            <SelectLabel>Outputs</SelectLabel>
            <SelectItem value="master">Master 1/2</SelectItem>
            <SelectItem value="cue">Headphones 3/4</SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  ),
};
