import {
  ContextMenuItem,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@opendeck/design-system";
import { useMemo, useState } from "react";
import type { PadMode } from "../../ipc/bindings";
import { useShiftHeld } from "../../hooks/useShiftHeld";
import { PADS_PER_LAYER, findEffect, formatDivision, padIndex } from "../../lib/fx";
import { useFxStore } from "../../state/fx";
import { padDown, padUp } from "../../state/pads";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { PadFxEditor } from "../fx/PadFxEditor";

interface PadFxViewProps {
  deck: number;
  mode: Extract<PadMode, "pad_fx1" | "pad_fx2">;
}

/**
 * Live pad FX pads of a deck: hold a pad to engage its effect, release to stop
 * it (a Release FX pad fires on release). SHIFT+click reaches the second
 * eight pads of the bank and the labels switch to them while SHIFT is down.
 * Assigning effects is in the pad's right-click menu.
 */
export function PadFxView({ deck, mode }: PadFxViewProps) {
  const bank = mode === "pad_fx2" ? 1 : 0;
  const shift = useShiftHeld();
  const [editing, setEditing] = useState(false);
  const pads = useFxStore((s) => s.state?.pads);
  const effects = useFxStore((s) => s.effects);

  const specs = useMemo<PadSpec[]>(
    () =>
      Array.from({ length: PADS_PER_LAYER }, (_, p) => {
        const assigned = pads?.[padIndex(bank, shift, p)];
        const effect = findEffect(effects, assigned?.effect ?? null);
        const detail = assigned?.release
          ? "release"
          : effect?.beat_synced
            ? formatDivision(assigned?.division)
            : "";
        return {
          label: effect ? effect.name : "Empty",
          sublabel: detail || undefined,
          ariaLabel: effect
            ? `Pad ${p + 1}, ${effect.name}${detail ? ` ${detail}` : ""}`
            : `Pad ${p + 1} (empty)`,
          color: null,
          lit: effect !== undefined,
        };
      }),
    [pads, effects, bank, shift],
  );

  return (
    <>
      <PerformancePads
        label={`Deck ${deck + 1} pad FX`}
        pads={specs}
        onPress={(index, { shift: shifted }) => padDown(deck, mode, index, shifted)}
        onRelease={(index) => padUp(deck, index, false)}
        renderMenu={() => (
          <ContextMenuItem
            aria-label={`Deck ${deck + 1} edit pad effects`}
            onSelect={() => setEditing(true)}
          >
            Assign effects...
          </ContextMenuItem>
        )}
      />
      <Dialog open={editing} onOpenChange={setEditing}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle>Pad FX</DialogTitle>
            <DialogDescription>
              32 pad effects: two banks, each with a shift layer. Hold a pad to engage, release to
              stop.
            </DialogDescription>
          </DialogHeader>
          <PadFxEditor />
        </DialogContent>
      </Dialog>
    </>
  );
}
