import { useMemo } from "react";
import { hotCuePress, hotCueRelease } from "../state/actions";
import { HOT_CUE_COUNT, selectDeckUi, useAppStore } from "../state/app";
import { cueToken } from "../viz/colors";
import { PerformancePads, type PadSpec } from "../viz/PerformancePads";

interface HotCuePadsProps {
  deck: number;
  disabled: boolean;
}

/**
 * Eight hot cue pads. Empty: click stores the playhead. Set: click jumps (hold
 * previews when paused). Shift+click on a set pad deletes it.
 */
export function HotCuePads({ deck, disabled }: HotCuePadsProps) {
  const { hotCues } = useAppStore(selectDeckUi(deck));
  const pads = useMemo<PadSpec[]>(
    () =>
      Array.from({ length: HOT_CUE_COUNT }, (_, index) => {
        const isSet = hotCues[index] != null;
        return {
          label: String(index + 1),
          ariaLabel: `Hot cue ${index + 1}${isSet ? "" : " (empty)"}`,
          color: cueToken(index),
          lit: isSet,
        };
      }),
    [hotCues],
  );
  return (
    <PerformancePads
      label="Hot cues"
      pads={pads}
      disabled={disabled}
      onPress={(index, { shift }) => hotCuePress(deck, index, shift)}
      onRelease={(index) => hotCueRelease(deck, index)}
    />
  );
}
