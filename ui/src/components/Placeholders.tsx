import { EmptyState, Panel, PanelBody, PanelHeader } from "@opendeck/design-system";
import { SlidersHorizontal } from "lucide-react";

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
