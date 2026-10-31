import { ContextMenuItem } from "@opendeck/design-system";
import { DEFAULT_SEMITONES, semitoneLabel } from "../../lib/pads";
import { setKeyShiftTo } from "../../state/actions";
import { selectDeckUi, useAppStore } from "../../state/app";
import { padDown, padUp } from "../../state/pads";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { useDeckLoaded, useDeckPads } from "./useDeckPads";

/** Pad index of SHIFT + pad 7 (layout up) and pad 8 (layout down), manual p.65. */
const LAYOUT_UP = 6;
const LAYOUT_DOWN = 7;

/** Semitone pads. The pad of the current key shift is lit. */
export function KeyShiftView({ deck }: { deck: number }) {
  const semitones = useDeckPads(deck)?.key_semitones ?? DEFAULT_SEMITONES;
  const { keyShift } = useAppStore(selectDeckUi(deck));
  const loaded = useDeckLoaded(deck);
  const pads: PadSpec[] = semitones.map((s) => ({
    label: semitoneLabel(s),
    sublabel: "st",
    ariaLabel: `Key shift ${semitoneLabel(s)} semitones`,
    color: null,
    lit: s === keyShift,
  }));
  const layout = (pad: number) => {
    padDown(deck, "key_shift", pad, true);
    padUp(deck, pad, true);
  };
  return (
    <PerformancePads
      label={`Deck ${deck + 1} key shift`}
      pads={pads}
      disabled={!loaded}
      onPress={(index, { shift }) => padDown(deck, "key_shift", index, shift)}
      onRelease={(index) => padUp(deck, index, false)}
      renderMenu={() => (
        <>
          <ContextMenuItem disabled={keyShift === 0} onSelect={() => void setKeyShiftTo(deck, 0)}>
            Reset key shift
          </ContextMenuItem>
          <ContextMenuItem
            aria-label={`Deck ${deck + 1} pad layout up`}
            onSelect={() => layout(LAYOUT_UP)}
          >
            Layout +1
          </ContextMenuItem>
          <ContextMenuItem
            aria-label={`Deck ${deck + 1} pad layout down`}
            onSelect={() => layout(LAYOUT_DOWN)}
          >
            Layout -1
          </ContextMenuItem>
        </>
      )}
    />
  );
}
