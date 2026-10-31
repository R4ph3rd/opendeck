import { Button } from "@opendeck/design-system";
import { Minus, Plus } from "lucide-react";
import { DEFAULT_JUMP_BEATS, jumpLabel } from "../../lib/pads";
import { padDown, padUp } from "../../state/pads";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { BeatJumpControls } from "../BeatJumpControls";
import { LoopControls } from "../LoopControls";
import { useDeckLoaded, useDeckPads } from "./useDeckPads";

/** Pad index of SHIFT + pad 7 (smaller jumps) and pad 8 (larger jumps), manual p.60. */
const SMALLER = 6;
const LARGER = 7;

/** Beat jump pads with per-pad sizes, the size scale, the jump buttons and loop controls. */
export function BeatJumpView({ deck }: { deck: number }) {
  const sizes = (useDeckPads(deck)?.jump_beats ?? DEFAULT_JUMP_BEATS).map((b) => b ?? 0);
  const loaded = useDeckLoaded(deck);
  const pads: PadSpec[] = sizes.map((beats) => ({
    label: jumpLabel(beats),
    ariaLabel: `Jump ${beats < 0 ? "back" : "forward"} ${Math.abs(beats)} beats`,
    color: null,
    lit: loaded,
  }));
  const scale = (pad: number) => {
    padDown(deck, "beat_jump", pad, true);
    padUp(deck, pad, true);
  };
  return (
    <div className="flex flex-col gap-2">
      <PerformancePads
        label={`Deck ${deck + 1} beat jump`}
        pads={pads}
        disabled={!loaded}
        onPress={(index) => padDown(deck, "beat_jump", index, false)}
        onRelease={(index) => padUp(deck, index, false)}
      />
      <div className="flex items-center gap-2">
        <span className="text-xs text-fg-muted">Pad sizes</span>
        <Button
          size="sm"
          aria-label={`Deck ${deck + 1} halve pad jump sizes`}
          onClick={() => scale(SMALLER)}
        >
          <Minus className="size-4" aria-hidden />
        </Button>
        <Button
          size="sm"
          aria-label={`Deck ${deck + 1} double pad jump sizes`}
          onClick={() => scale(LARGER)}
        >
          <Plus className="size-4" aria-hidden />
        </Button>
      </div>
      <BeatJumpControls deck={deck} disabled={!loaded} />
      <LoopControls deck={deck} disabled={!loaded} />
    </div>
  );
}
