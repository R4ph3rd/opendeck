import type { Meta, StoryObj } from "@storybook/react-vite";
import { Button } from "./button";
import { Input } from "./input";
import { HoverCard, HoverCardContent, HoverCardTrigger, Popover, PopoverContent, PopoverTrigger } from "./popover";

const meta: Meta<typeof Popover> = {
  title: "Components/Popover",
  component: Popover,
  parameters: {
    docs: {
      description: {
        component: [
          "`Popover` opens on click and holds interactive content (small forms, pickers). Focus moves in; Escape closes and returns focus to the trigger.",
          "",
          "`HoverCard` previews on hover and keyboard focus (track info, cue details). Do not put the only path to an action inside it.",
          "",
          "For lists of actions use DropdownMenu. For a searchable list use Combobox.",
        ].join("\n"),
      },
    },
  },
};
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="outline">Rename cue</Button>
      </PopoverTrigger>
      <PopoverContent className="flex flex-col gap-2">
        <label className="text-xs font-medium text-fg-muted" htmlFor="cue-name">
          Cue name
        </label>
        <Input id="cue-name" defaultValue="Drop" />
      </PopoverContent>
    </Popover>
  ),
};

export const Hover: Story = {
  render: () => (
    <HoverCard openDelay={150}>
      <HoverCardTrigger asChild>
        <Button variant="ghost">Strobe - deadmau5</Button>
      </HoverCardTrigger>
      <HoverCardContent className="flex flex-col gap-1">
        <p className="font-semibold">Strobe</p>
        <p className="text-fg-muted">128.0 BPM, 8A, 10:33</p>
      </HoverCardContent>
    </HoverCard>
  ),
};
