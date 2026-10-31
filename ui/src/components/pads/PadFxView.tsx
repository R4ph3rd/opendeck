import { Toggle } from "@opendeck/design-system";
import { useMemo, useState } from "react";
import type { PadMode } from "../../ipc/bindings";
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
 * it (a Release FX pad fires on release). The SHIFT layer toggle reaches the
 * second eight pads of the bank, like holding SHIFT on the controller.
 */
export function PadFxView({ deck, mode }: PadFxViewProps) {
  const bank = mode === "pad_fx2" ? 1 : 0;
  const [shift, setShift] = useState(false);
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
          label: effect ? effect.name : String(p + 1),
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
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Toggle
          variant="outline"
          size="sm"
          aria-label={`Deck ${deck + 1} pad FX shift layer`}
          pressed={shift}
          onPressedChange={setShift}
        >
          Shift layer
        </Toggle>
        <Toggle
          variant="outline"
          size="sm"
          aria-label={`Deck ${deck + 1} edit pad effects`}
          pressed={editing}
          onPressedChange={setEditing}
        >
          Assign
        </Toggle>
      </div>
      <PerformancePads
        label={`Deck ${deck + 1} pad FX`}
        pads={specs}
        onPress={(index) => padDown(deck, mode, index, shift)}
        onRelease={(index) => padUp(deck, index, shift)}
      />
      {editing ? <PadFxEditor /> : null}
    </div>
  );
}
