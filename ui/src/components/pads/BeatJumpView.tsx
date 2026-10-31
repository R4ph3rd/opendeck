import { ContextMenuItem } from "@opendeck/design-system";
import { DEFAULT_JUMP_BEATS, jumpLabel } from "../../lib/pads";
import { padDown, padUp } from "../../state/pads";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { useDeckLoaded, useDeckPads } from "./useDeckPads";

/** Pad index of SHIFT + pad 7 (smaller jumps) and pad 8 (larger jumps), manual p.60. */
const SMALLER = 6;
const LARGER = 7;

/** Beat jump pads, one size each. SHIFT+click is the shifted action; the menu halves or doubles every size. */
export function BeatJumpView({ deck }: { deck: number }) {
  const sizes = (useDeckPads(deck)?.jump_beats ?? DEFAULT_JUMP_BEATS).map((b) => b ?? 0);
  const loaded = useDeckLoaded(deck);
  const pads: PadSpec[] = sizes.map((beats) => ({
    label: jumpLabel(beats),
    sublabel: "beats",
    ariaLabel: `Jump ${beats < 0 ? "back" : "forward"} ${Math.abs(beats)} beats`,
    color: null,
    lit: loaded,
  }));
  const scale = (pad: number) => {
    padDown(deck, "beat_jump", pad, true);
    padUp(deck, pad, true);
  };
  return (
    <PerformancePads
      label={`Deck ${deck + 1} beat jump`}
      pads={pads}
      disabled={!loaded}
      onPress={(index, { shift }) => padDown(deck, "beat_jump", index, shift)}
      onRelease={(index) => padUp(deck, index, false)}
      renderMenu={() => (
        <>
          <ContextMenuItem
            aria-label={`Deck ${deck + 1} halve pad jump sizes`}
            onSelect={() => scale(SMALLER)}
          >
            Halve all jump sizes
          </ContextMenuItem>
          <ContextMenuItem
            aria-label={`Deck ${deck + 1} double pad jump sizes`}
            onSelect={() => scale(LARGER)}
          >
            Double all jump sizes
          </ContextMenuItem>
        </>
      )}
    />
  );
}
