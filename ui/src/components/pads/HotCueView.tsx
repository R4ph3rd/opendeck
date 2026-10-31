import { ContextMenuItem, ContextMenuSeparator } from "@opendeck/design-system";
import { useMemo, useState } from "react";
import { commands } from "../../ipc/bindings";
import { formatTime } from "../../lib/format";
import { report } from "../../state/actions";
import { HOT_CUE_COUNT, selectDeckUi, useAppStore } from "../../state/app";
import { padDown, padUp, renameHotCue } from "../../state/pads";
import { cueToken } from "../../viz/colors";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { RenameDialog } from "./RenameDialog";
import { useDeckLoaded } from "./useDeckPads";

/**
 * Eight hot cue pads. An empty pad stores the playhead, a set pad jumps to it
 * (hold to preview when paused), SHIFT+click deletes it. Name and time sit on
 * the pad; rename, set and delete are in the right-click menu.
 */
export function HotCueView({ deck }: { deck: number }) {
  const { hotCues } = useAppStore(selectDeckUi(deck));
  const loaded = useDeckLoaded(deck);
  const [renaming, setRenaming] = useState<number | null>(null);

  const pads = useMemo<PadSpec[]>(
    () =>
      Array.from({ length: HOT_CUE_COUNT }, (_, index) => {
        const cue = hotCues[index] ?? null;
        return {
          label: cue ? cue.name || `Cue ${index + 1}` : "Empty",
          sublabel: cue ? formatTime(cue.start ?? 0) : undefined,
          ariaLabel: `Hot cue ${index + 1}${cue ? "" : " (empty)"}`,
          color: cueToken(index),
          lit: cue !== null,
        };
      }),
    [hotCues],
  );

  return (
    <>
      <PerformancePads
        label={`Deck ${deck + 1} hot cues`}
        pads={pads}
        disabled={!loaded}
        onPress={(index, { shift }) => padDown(deck, "hot_cue", index, shift)}
        onRelease={(index) => padUp(deck, index, false)}
        renderMenu={(index) => {
          const cue = hotCues[index] ?? null;
          return (
            <>
              <ContextMenuItem
                disabled={!loaded}
                onSelect={() => void report(commands.setHotCue(deck, index))}
              >
                {cue ? "Move to playhead" : "Set at playhead"}
              </ContextMenuItem>
              <ContextMenuItem disabled={cue === null} onSelect={() => setRenaming(index)}>
                Rename...
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                disabled={cue === null}
                onSelect={() => void report(commands.deleteHotCue(deck, index))}
              >
                Delete
              </ContextMenuItem>
            </>
          );
        }}
      />
      <RenameDialog
        open={renaming !== null}
        onOpenChange={(open) => {
          if (!open) setRenaming(null);
        }}
        title={`Rename hot cue ${(renaming ?? 0) + 1}`}
        initial={renaming === null ? "" : (hotCues[renaming]?.name ?? "")}
        onCommit={(name) => {
          if (renaming !== null) renameHotCue(deck, renaming, name);
        }}
      />
    </>
  );
}
