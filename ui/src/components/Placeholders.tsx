import { EmptyState, Panel, PanelBody, PanelHeader } from "@opendeck/design-system";
import { Disc3, Library, SlidersHorizontal } from "lucide-react";

/** Deck B slot: reserved until the second deck and mixer land (milestone 2). */
export function DeckSlot({ deck }: { deck: number }) {
  return (
    <Panel aria-label={`Deck ${deck + 1}`} className="border-t-2 border-t-deck-b">
      <PanelHeader title={`Deck ${deck + 1}`} />
      <PanelBody className="flex">
        <EmptyState
          className="flex-1"
          icon={<Disc3 aria-hidden />}
          title="Deck unavailable"
          description="The second deck and the mixer arrive in the next milestone."
        />
      </PanelBody>
    </Panel>
  );
}

/** Middle panel: follows the active pad mode per deck from milestone 4. */
export function MiddlePanel() {
  return (
    <Panel aria-label="Pads and effects" className="h-32 shrink-0">
      <PanelHeader title="Pads" />
      <PanelBody className="flex">
        <EmptyState
          className="flex-1"
          icon={<SlidersHorizontal aria-hidden />}
          title="Hot cues, loops and effects"
          description="This panel follows the pad mode of each deck."
        />
      </PanelBody>
    </Panel>
  );
}

/** Collection: tracks and playlists arrive with the library (milestone 3). */
export function CollectionPanel() {
  return (
    <Panel aria-label="Collection" className="min-h-0 flex-1">
      <PanelHeader title="Collection" />
      <PanelBody className="flex">
        <EmptyState
          className="flex-1"
          icon={<Library aria-hidden />}
          title="Your collection is empty"
          description="Importing folders and playlists arrives with the library."
        />
      </PanelBody>
    </Panel>
  );
}
