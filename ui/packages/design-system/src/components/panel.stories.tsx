import type { Meta, StoryObj } from "@storybook/react-vite";
import { Settings } from "lucide-react";
import { IconButton } from "./icon-button";
import { Panel, PanelBody, PanelHeader } from "./panel";

const meta = { title: "Components/Panel", component: Panel } satisfies Meta<typeof Panel>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Panel className="w-80">
      <PanelHeader
        title="Deck A"
        actions={
          <IconButton size="sm" aria-label="Deck settings">
            <Settings className="size-3.5" />
          </IconButton>
        }
      />
      <PanelBody className="text-sm text-fg-muted">Panel content.</PanelBody>
    </Panel>
  ),
};
