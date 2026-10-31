import { Badge } from "@opendeck/design-system";
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
    ariaLabel: `Loop ${beatsLabel(beats)} beats`,
    color: null,
    lit: active !== null && Math.abs(active - beats) < 0.02,
  }));
  return (
    <div className="flex flex-col gap-2">
      <PerformancePads
        label={`Deck ${deck + 1} beat loop`}
        pads={pads}
        disabled={!loaded}
        onPress={(index) => padDown(deck, "beat_loop", index, false)}
        onRelease={(index) => padUp(deck, index, false)}
      />
      <p className="flex items-center gap-2 text-sm text-fg-muted">
        Active loop
        <Badge variant={active === null ? "neutral" : "accent"} data-testid={`active-loop-${deck}`}>
          {active === null ? "none" : `${beatsLabel(Math.round(active * 4) / 4)} beats`}
        </Badge>
      </p>
    </div>
  );
}
