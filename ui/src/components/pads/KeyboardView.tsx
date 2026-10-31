import { ContextMenuItem } from "@opendeck/design-system";
import { DEFAULT_SEMITONES, semitoneLabel } from "../../lib/pads";
import { selectDeckUi, useAppStore } from "../../state/app";
import { padDown, padUp, selectPadMode } from "../../state/pads";
import { cueToken } from "../../viz/colors";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { useDeckLoaded, useDeckPads } from "./useDeckPads";

/** Pad index of SHIFT + pad 7 (pitch up) and pad 8 (pitch down), manual p.63. */
const PITCH_UP = 6;
const PITCH_DOWN = 7;

/**
 * KEYBOARD: the first press picks the hot cue (an empty pad stores the
 * playhead); then the pads play it at semitone offsets.
 */
export function KeyboardView({ deck }: { deck: number }) {
  const pads = useDeckPads(deck);
  const { hotCues } = useAppStore(selectDeckUi(deck));
  const loaded = useDeckLoaded(deck);
  const cue = pads?.keyboard_cue ?? null;
  const semitones = pads?.keyboard_semitones ?? DEFAULT_SEMITONES;

  const specs: PadSpec[] =
    cue === null
      ? Array.from({ length: 8 }, (_, i) => ({
          label: hotCues[i]?.name || `Cue ${i + 1}`,
          sublabel: hotCues[i] != null ? "choose" : "set here",
          ariaLabel: `Choose hot cue ${i + 1}`,
          color: cueToken(i),
          lit: hotCues[i] != null,
        }))
      : semitones.map((s) => ({
          label: semitoneLabel(s),
          sublabel: `cue ${cue + 1}`,
          ariaLabel: `Play cue ${cue + 1} at ${semitoneLabel(s)} semitones`,
          color: cueToken(cue),
          lit: true,
        }));
  const shifted = (pad: number) => {
    padDown(deck, "keyboard", pad, true);
    padUp(deck, pad, true);
  };

  return (
    <PerformancePads
      label={`Deck ${deck + 1} keyboard`}
      pads={specs}
      disabled={!loaded}
      onPress={(index, { shift }) => padDown(deck, "keyboard", index, shift)}
      onRelease={(index) => padUp(deck, index, false)}
      renderMenu={() => (
        <>
          <ContextMenuItem disabled={cue === null} onSelect={() => selectPadMode(deck, "keyboard")}>
            Choose another cue
          </ContextMenuItem>
          <ContextMenuItem
            disabled={cue === null}
            aria-label={`Deck ${deck + 1} pitch layout up`}
            onSelect={() => shifted(PITCH_UP)}
          >
            Pitch +1
          </ContextMenuItem>
          <ContextMenuItem
            disabled={cue === null}
            aria-label={`Deck ${deck + 1} pitch layout down`}
            onSelect={() => shifted(PITCH_DOWN)}
          >
            Pitch -1
          </ContextMenuItem>
          <ContextMenuItem
            disabled={cue === null}
            aria-label={`Deck ${deck + 1} delete chosen cue`}
            onSelect={() => shifted(0)}
          >
            Delete cue
          </ContextMenuItem>
        </>
      )}
    />
  );
}
