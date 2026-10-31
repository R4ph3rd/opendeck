import { Button } from "@opendeck/design-system";
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
  const selected = cue === null ? null : (hotCues[cue] ?? null);

  const specs: PadSpec[] =
    cue === null
      ? Array.from({ length: 8 }, (_, i) => ({
          label: String(i + 1),
          ariaLabel: `Choose hot cue ${i + 1}`,
          color: cueToken(i),
          lit: hotCues[i] != null,
        }))
      : semitones.map((s) => ({
          label: semitoneLabel(s),
          ariaLabel: `Play cue ${cue + 1} at ${semitoneLabel(s)} semitones`,
          color: cueToken(cue),
          lit: true,
        }));
  const shifted = (pad: number) => {
    padDown(deck, "keyboard", pad, true);
    padUp(deck, pad, true);
  };

  return (
    <div className="flex flex-col gap-2">
      <p className="flex items-center gap-2 text-sm text-fg-muted" data-testid={`keyboard-cue-${deck}`}>
        {cue === null ? (
          "Press a pad to choose the hot cue."
        ) : (
          <>
            <span
              aria-hidden
              className="size-3 rounded-sm"
              style={{ backgroundColor: `var(--${cueToken(cue)})` }}
            />
            <span className="text-fg">{`Cue ${cue + 1}${selected?.name ? `, ${selected.name}` : ""}`}</span>
          </>
        )}
      </p>
      <PerformancePads
        label={`Deck ${deck + 1} keyboard`}
        pads={specs}
        disabled={!loaded}
        onPress={(index) => padDown(deck, "keyboard", index, false)}
        onRelease={(index) => padUp(deck, index, false)}
      />
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={cue === null} onClick={() => selectPadMode(deck, "keyboard")}>
          Choose another cue
        </Button>
        <Button
          size="sm"
          disabled={cue === null}
          aria-label={`Deck ${deck + 1} pitch layout up`}
          onClick={() => shifted(PITCH_UP)}
        >
          Pitch +1
        </Button>
        <Button
          size="sm"
          disabled={cue === null}
          aria-label={`Deck ${deck + 1} pitch layout down`}
          onClick={() => shifted(PITCH_DOWN)}
        >
          Pitch -1
        </Button>
        <Button
          size="sm"
          disabled={cue === null}
          aria-label={`Deck ${deck + 1} delete chosen cue`}
          onClick={() => shifted(0)}
        >
          Delete cue
        </Button>
      </div>
    </div>
  );
}
