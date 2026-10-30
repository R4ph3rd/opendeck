import type { Meta, StoryObj } from "@storybook/react-vite";
import { Disc3, Settings, Upload } from "lucide-react";
import { useState } from "react";
import { Button } from "./button";
import {
  Combobox,
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandPalette,
  CommandSeparator,
  CommandShortcut,
} from "./command";
import { Kbd } from "./kbd";

const meta: Meta<typeof Command> = {
  title: "Components/Command",
  component: Command,
  parameters: {
    docs: {
      description: {
        component: [
          "Built on `cmdk` (MIT). `CommandPalette` is the modal quick search and command launcher: bind it to Mod+K in the app. `Command` alone is the inline list; `Combobox` is a searchable single select.",
          "",
          "- Typing filters and ranks items; arrows move, Enter runs, Escape closes.",
          "- Group related items with `CommandGroup` and a heading. Put the shortcut in `CommandShortcut`.",
          "- `Combobox` takes `{ value, label, keywords? }` options. Choosing the selected option again clears it.",
        ].join("\n"),
      },
    },
  },
};
export default meta;
type Story = StoryObj<typeof meta>;

export const Inline: Story = {
  render: () => (
    <Command className="w-96 rounded-lg border border-border">
      <CommandInput placeholder="Search tracks and commands" />
      <CommandList>
        <CommandEmpty>No results.</CommandEmpty>
        <CommandGroup heading="Commands">
          <CommandItem>
            <Upload className="size-4" aria-hidden /> Import folder
          </CommandItem>
          <CommandItem>
            <Settings className="size-4" aria-hidden /> Open settings
            <CommandShortcut>
              <Kbd>Ctrl</Kbd>
              <Kbd>,</Kbd>
            </CommandShortcut>
          </CommandItem>
        </CommandGroup>
        <CommandSeparator />
        <CommandGroup heading="Tracks">
          <CommandItem>
            <Disc3 className="size-4" aria-hidden /> Strobe
          </CommandItem>
          <CommandItem disabled>
            <Disc3 className="size-4" aria-hidden /> Missing file
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </Command>
  ),
};

export const Palette: Story = {
  render: function Render() {
    const [open, setOpen] = useState(false);
    return (
      <>
        <Button variant="outline" onClick={() => setOpen(true)}>
          Open palette
        </Button>
        <CommandPalette open={open} onOpenChange={setOpen}>
          <CommandInput placeholder="Type a command" />
          <CommandList>
            <CommandEmpty>No results.</CommandEmpty>
            <CommandGroup heading="Commands">
              <CommandItem onSelect={() => setOpen(false)}>Load selected to deck A</CommandItem>
              <CommandItem onSelect={() => setOpen(false)}>Load selected to deck B</CommandItem>
              <CommandItem onSelect={() => setOpen(false)}>Open settings</CommandItem>
            </CommandGroup>
          </CommandList>
        </CommandPalette>
      </>
    );
  },
};

export const ComboboxStory: Story = {
  name: "Combobox",
  render: function Render() {
    const [value, setValue] = useState<string | undefined>();
    return (
      <Combobox
        aria-label="Audio output"
        placeholder="Choose output"
        value={value}
        onValueChange={setValue}
        options={[
          { value: "builtin", label: "Built-in output", keywords: ["speakers"] },
          { value: "usb", label: "USB audio interface", keywords: ["dac"] },
          { value: "hdmi", label: "HDMI", disabled: true },
        ]}
      />
    );
  },
};
