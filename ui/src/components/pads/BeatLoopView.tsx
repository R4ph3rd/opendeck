import { DEFAULT_LOOP_BEATS, beatsLabel } from "../../lib/pads";
import { padDown, padUp } from "../../state/pads";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { useActiveLoopBeats, useDeckLoaded, useDeckPads } from "./useDeckPads";

/** Loop length of each BEAT LOOP pad; the pad of the active loop is lit, press it again to exit. */
export function BeatLoopView({ deck }: { deck: number }) {
  const lengths = (useDeckPads(deck)?.loop_beats ?? DEFAULT_LOOP_BEATS).map((b) => b ?? 0);
  const loaded = useDeckLoaded(deck);
  const active = useActiveLoopBeats(deck);
  const pads: PadSpec[] = lengths.map((beats) => ({
    label: beatsLabel(beats),
    sublabel: "beats",
    ariaLabel: `Loop ${beatsLabel(beats)} beats`,
    color: null,
    lit: active !== null && Math.abs(active - beats) < 0.02,
  }));
  return (
    <PerformancePads
      label={`Deck ${deck + 1} beat loop`}
      pads={pads}
      disabled={!loaded}
      onPress={(index, { shift }) => padDown(deck, "beat_loop", index, shift)}
      onRelease={(index) => padUp(deck, index, false)}
    />
  );
}
