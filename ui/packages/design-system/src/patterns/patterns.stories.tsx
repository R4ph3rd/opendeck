import type { Meta, StoryObj } from "@storybook/react-vite";
import { Disc3, FolderPlus, Headphones, Keyboard, Library, Repeat, Settings } from "lucide-react";
import { useState } from "react";
import { Button } from "../components/button";
import { Callout } from "../components/callout";
import { EmptyState } from "../components/empty-state";
import { IconButton } from "../components/icon-button";
import { KeyValueList } from "../components/key-value";
import { NumberField } from "../components/number-field";
import { Panel, PanelBody, PanelHeader } from "../components/panel";
import { SegmentedControl } from "../components/segmented-control";
import { SettingsLayout, SettingsNav, SettingsRow, SettingsSection } from "../components/settings-layout";
import { StatusDot } from "../components/status-dot";
import { Switch } from "../components/switch";

const meta: Meta = {
  title: "Patterns/Overview",
  parameters: {
    docs: {
      description: {
        component:
          "Compositions of design system parts. Copy the structure, not the data. Every pattern uses tokens only and works in both themes.",
      },
    },
  },
};
export default meta;
type Story = StoryObj;

/** Deck header: title, sync and master LEDs, BPM entry, tempo range, loop toggle. One row, 32px tall controls at most. */
export const DensePanelHeader: Story = {
  render: () => (
    <Panel className="w-176">
      <PanelHeader
        title={<span className="text-deck-a">Deck A</span>}
        className="h-9 gap-3"
        actions={
          <>
            <StatusDot label="SYNC" tone="sync" />
            <StatusDot label="MASTER" tone="master" state="off" />
            <NumberField aria-label="Deck A BPM" defaultValue={128} min={20} max={300} step={0.01} largeStep={1} unit="BPM" className="ml-2" />
            <SegmentedControl
              size="sm"
              aria-label="Deck A tempo range"
              defaultValue="8"
              options={[
                { value: "6", label: "6%" },
                { value: "8", label: "8%" },
                { value: "16", label: "16%" },
              ]}
            />
            <IconButton size="sm" aria-label="Loop">
              <Repeat className="size-3.5" />
            </IconButton>
            <IconButton size="sm" aria-label="Deck settings">
              <Settings className="size-3.5" />
            </IconButton>
          </>
        }
      />
      <PanelBody className="h-24 text-sm text-fg-muted">Waveform area</PanelBody>
    </Panel>
  ),
};

/** First run or empty list: icon, one sentence, one action. Flat, no decoration. */
export const EmptyStateWithBackdrop: Story = {
  render: () => (
    <Panel className="h-80 w-128">
      <PanelHeader title="Collection" />
      <EmptyState
        className="flex-1"
        icon={<Disc3 />}
        title="No tracks yet"
        description="Add a folder and OpenDeck scans it for music."
        action={
          <Button variant="primary">
            <FolderPlus className="size-4" aria-hidden /> Add folder
          </Button>
        }
      />
    </Panel>
  ),
};

/** Settings page: nav + sections of rows, an info callout and a read-only key/value block. */
export const SettingsPageSkeleton: Story = {
  render: function Render() {
    const [page, setPage] = useState("audio");
    return (
      <div className="h-120 w-192 overflow-hidden rounded-lg border border-border">
        <SettingsLayout
          sidebar={
            <SettingsNav
              value={page}
              onValueChange={setPage}
              items={[
                { id: "audio", label: "Audio", icon: <Headphones /> },
                { id: "midi", label: "MIDI", icon: <Keyboard /> },
                { id: "library", label: "Library", icon: <Library /> },
              ]}
            />
          }
        >
          <SettingsSection id="general" title="General" description="Applies to every deck.">
            <SettingsRow title="Auto sync on load" description="Match the master tempo when a track loads" control={<Switch aria-label="Auto sync on load" defaultChecked />} />
            <SettingsRow title="Tempo range" control={<SegmentedControl aria-label="Default tempo range" defaultValue="8" options={[{ value: "8", label: "8%" }, { value: "16", label: "16%" }]} />} />
          </SettingsSection>
          <Callout variant="info">Changes save as you make them.</Callout>
          <SettingsSection id="about" title="About">
            <KeyValueList items={[{ label: "Version", value: "0.1.0" }, { label: "Audio backend", value: "cpal" }]} />
          </SettingsSection>
        </SettingsLayout>
      </div>
    );
  },
};
