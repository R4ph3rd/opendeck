import { Button, cn } from "@opendeck/design-system";
import type { KeyboardEvent, PointerEvent } from "react";
import { CUE_FILL, CUE_TEXT } from "../lib/deck";
import { hotCuePress, hotCueRelease } from "../state/actions";
import { HOT_CUE_COUNT, selectDeckUi, useAppStore } from "../state/app";

interface HotCuePadsProps {
  deck: number;
  disabled: boolean;
}

const SLOTS = Array.from({ length: HOT_CUE_COUNT }, (_, i) => i);

/**
 * Eight hot cue pads. Empty: click stores the playhead. Set: click jumps (hold
 * previews when paused). Shift+click on a set pad deletes it.
 */
export function HotCuePads({ deck, disabled }: HotCuePadsProps) {
  const { hotCues } = useAppStore(selectDeckUi(deck));

  const onDown = (event: PointerEvent<HTMLButtonElement>, index: number) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    hotCuePress(deck, index, event.shiftKey);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if ((event.key === " " || event.key === "Enter") && !event.repeat) {
      event.preventDefault();
      hotCuePress(deck, index, event.shiftKey);
    }
  };

  return (
    <div className="grid grid-cols-4 gap-1" role="group" aria-label="Hot cues">
      {SLOTS.map((index) => {
        const isSet = hotCues[index] != null;
        return (
          <Button
            key={index}
            size="md"
            aria-label={`Hot cue ${index + 1}${isSet ? "" : " (empty)"}`}
            aria-pressed={isSet}
            disabled={disabled}
            className={cn(
              "border",
              isSet ? `${CUE_FILL[index]} border-transparent text-cue-fg hover:opacity-90` : `border-border ${CUE_TEXT[index]}`,
            )}
            onPointerDown={(event) => onDown(event, index)}
            onPointerUp={() => hotCueRelease(deck, index)}
            onPointerCancel={() => hotCueRelease(deck, index)}
            onLostPointerCapture={() => hotCueRelease(deck, index)}
            onKeyDown={(event) => onKeyDown(event, index)}
            onKeyUp={() => hotCueRelease(deck, index)}
          >
            {index + 1}
          </Button>
        );
      })}
    </div>
  );
}
