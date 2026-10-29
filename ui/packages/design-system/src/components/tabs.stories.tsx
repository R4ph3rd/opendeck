import type { Meta, StoryObj } from "@storybook/react-vite";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./tabs";

const meta = { title: "Components/Tabs", component: Tabs } satisfies Meta<typeof Tabs>;
export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <Tabs defaultValue="library">
      <TabsList>
        <TabsTrigger value="library">Library</TabsTrigger>
        <TabsTrigger value="crates">Crates</TabsTrigger>
        <TabsTrigger value="history" disabled>
          History
        </TabsTrigger>
      </TabsList>
      <TabsContent value="library" className="text-sm text-fg-muted">
        All tracks.
      </TabsContent>
      <TabsContent value="crates" className="text-sm text-fg-muted">
        Your crates.
      </TabsContent>
    </Tabs>
  ),
};
