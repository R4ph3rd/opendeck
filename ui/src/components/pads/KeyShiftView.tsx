import { Badge, Button } from "@opendeck/design-system";
import { DEFAULT_SEMITONES, semitoneLabel } from "../../lib/pads";
import { commands } from "../../ipc/bindings";
import { hydrateDeck, report } from "../../state/actions";
import { selectDeckUi, useAppStore } from "../../state/app";
import { padDown, padUp } from "../../state/pads";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { useDeckLoaded, useDeckPads } from "./useDeckPads";

/** Pad index of SHIFT + pad 7 (layout up) and pad 8 (layout down), manual p.65. */
const LAYOUT_UP = 6;
const LAYOUT_DOWN = 7;

/** Semitone pads, the current key shift and a reset. */
export function KeyShiftView({ deck }: { deck: number }) {
  const semitones = useDeckPads(deck)?.key_semitones ?? DEFAULT_SEMITONES;
  const { keyShift } = useAppStore(selectDeckUi(deck));
  const loaded = useDeckLoaded(deck);
  const pads: PadSpec[] = semitones.map((s) => ({
    label: semitoneLabel(s),
    ariaLabel: `Key shift ${semitoneLabel(s)} semitones`,
    color: null,
    lit: s === keyShift,
  }));
  const layout = (pad: number) => {
    padDown(deck, "key_shift", pad, true);
    padUp(deck, pad, true);
  };
  return (
    <div className="flex flex-col gap-2">
      <PerformancePads
        label={`Deck ${deck + 1} key shift`}
        pads={pads}
        disabled={!loaded}
        onPress={(index) => padDown(deck, "key_shift", index, false)}
        onRelease={(index) => padUp(deck, index, false)}
      />
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-fg-muted">Key shift</span>
        <Badge variant={keyShift === 0 ? "neutral" : "accent"} data-testid={`key-shift-${deck}`}>
          {semitoneLabel(keyShift)} st
        </Badge>
        <Button
          size="sm"
          disabled={!loaded || keyShift === 0}
          onClick={() => {
            void report(commands.setKeyShift(deck, 0)).then(() => hydrateDeck(deck));
          }}
        >
          Reset
        </Button>
        <Button size="sm" aria-label={`Deck ${deck + 1} pad layout up`} onClick={() => layout(LAYOUT_UP)}>
          Layout +1
        </Button>
        <Button size="sm" aria-label={`Deck ${deck + 1} pad layout down`} onClick={() => layout(LAYOUT_DOWN)}>
          Layout -1
        </Button>
      </div>
    </div>
  );
}
