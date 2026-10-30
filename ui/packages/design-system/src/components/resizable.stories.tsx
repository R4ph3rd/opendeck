import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Panel, PanelBody, PanelHeader } from "./panel";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  type ResizableLayout,
} from "./resizable";

const meta: Meta<typeof ResizablePanelGroup> = {
  title: "Components/Resizable",
  component: ResizablePanelGroup,
  parameters: {
    docs: {
      description: {
        component: [
          "Split panes built on `react-resizable-panels` (MIT). Use for the deck, middle panel and collection split.",
          "",
          "- Give every `ResizablePanel` an `id`. Layout keys are those ids.",
          "- Persist with `onLayoutChanged` (fires once a resize settles, not on every pixel) and restore with `defaultLayout`.",
          "- Numbers in `minSize`/`maxSize` are pixels, strings are percentages (`\"20%\"`).",
          "- Handles are keyboard focusable: arrows resize, Home and End jump to the limits, double click resets.",
          "- Nest a vertical group inside a horizontal one for a three-way split.",
        ].join("\n"),
      },
    },
  },
};
export default meta;
type Story = StoryObj<typeof meta>;

function Pane({ title, children }: { title: string; children?: string }) {
  return (
    <Panel className="size-full rounded-none border-0">
      <PanelHeader title={title} />
      <PanelBody className="text-sm text-fg-muted">{children}</PanelBody>
    </Panel>
  );
}

export const Horizontal: Story = {
  render: () => (
    <div className="h-64 rounded-lg border border-border">
      <ResizablePanelGroup orientation="horizontal">
        <ResizablePanel id="left" defaultSize="30%" minSize="15%">
          <Pane title="Left">Drag the handle.</Pane>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel id="right" minSize="30%">
          <Pane title="Right" />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  ),
};

/** Deck on top, collection below, a side panel on the right; sizes reported through the callback. */
export const DeckAndCollectionPersisted: Story = {
  render: function Render() {
    const [saved, setSaved] = useState<ResizableLayout>({});
    const [vertical, setVertical] = useState<ResizableLayout>({});
    return (
      <div className="flex flex-col gap-2">
        <div className="h-96 rounded-lg border border-border">
          <ResizablePanelGroup orientation="horizontal" onLayoutChanged={setSaved}>
            <ResizablePanel id="main" minSize="40%">
              <ResizablePanelGroup orientation="vertical" onLayoutChanged={setVertical}>
                <ResizablePanel id="decks" defaultSize="45%" minSize="20%">
                  <Pane title="Decks" />
                </ResizablePanel>
                <ResizableHandle withGrip />
                <ResizablePanel id="collection" minSize="20%">
                  <Pane title="Collection" />
                </ResizablePanel>
              </ResizablePanelGroup>
            </ResizablePanel>
            <ResizableHandle withGrip />
            <ResizablePanel id="middle" defaultSize="25%" minSize="15%" maxSize="45%">
              <Pane title="Middle panel" />
            </ResizablePanel>
          </ResizablePanelGroup>
        </div>
        <pre className="rounded-md bg-bg-subtle p-2 font-mono text-xs text-fg-muted">
          {JSON.stringify({ horizontal: saved, vertical }, null, 2)}
        </pre>
      </div>
    );
  },
};

export const RestoredLayout: Story = {
  render: () => (
    <div className="h-48 rounded-lg border border-border">
      <ResizablePanelGroup orientation="horizontal" defaultLayout={{ a: 70, b: 30 }}>
        <ResizablePanel id="a">
          <Pane title="A (70)" />
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel id="b">
          <Pane title="B (30)" />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  ),
};
