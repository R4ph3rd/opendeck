import type { Meta, StoryObj } from "@storybook/react-vite";
import { Headphones, Keyboard, Library } from "lucide-react";
import { useState } from "react";
import { Callout } from "./callout";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./select";
import { SettingsLayout, SettingsNav, SettingsRow, SettingsSection } from "./settings-layout";
import { Switch } from "./switch";

const meta: Meta<typeof SettingsLayout> = {
  title: "Components/SettingsLayout",
  component: SettingsLayout,
  parameters: {
    docs: {
      description: {
        component: [
          "Frame for the full settings dialog: `SettingsLayout` (sidebar + scrolling pane), `SettingsNav` (section list, `aria-current=\"page\"` on the active one), `SettingsSection` (heading + rows) and `SettingsRow` (title, hint, control on the right).",
          "",
          "Give it a fixed height from the parent (for example inside a Dialog). The pane scrolls, the sidebar stays. Keep one section per nav item.",
        ].join("\n"),
      },
    },
  },
};
export default meta;
type Story = StoryObj<typeof meta>;

const items = [
  { id: "audio", label: "Audio", icon: <Headphones /> },
  { id: "midi", label: "MIDI", icon: <Keyboard /> },
  { id: "library", label: "Library", icon: <Library /> },
];

export const Default: Story = {
  render: function Render() {
    const [value, setValue] = useState("audio");
    return (
      <div className="h-112 overflow-hidden rounded-lg border border-border">
        <SettingsLayout sidebar={<SettingsNav items={items} value={value} onValueChange={setValue} />}>
          <SettingsSection id={value} title={items.find((i) => i.id === value)?.label} description="Applies to all decks.">
            <SettingsRow
              title="Output device"
              description="Master output"
              control={
                <Select defaultValue="builtin">
                  <SelectTrigger aria-label="Output device" className="w-44">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="builtin">Built-in output</SelectItem>
                    <SelectItem value="usb">USB interface</SelectItem>
                  </SelectContent>
                </Select>
              }
            />
            <SettingsRow
              title="Exclusive mode"
              description="Lower latency, other apps go silent"
              control={<Switch aria-label="Exclusive mode" />}
            />
          </SettingsSection>
          <Callout variant="info">Restart is not needed.</Callout>
        </SettingsLayout>
      </div>
    );
  },
};
