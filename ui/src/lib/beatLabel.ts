import { Badge, Button, Toggle } from "@opendeck/design-system";
import { useRef, type PointerEvent } from "react";
import { useStore } from "zustand";
import {
  autoLoop,
  loopIn,
  loopOut,
  loopRollPress,
  loopRollRelease,
  loopToggle,
  resizeLoop,
  setRollMode,
} from "../state/actions";
import { LOOP_BEATS, selectDeckUi, useAppStore } from "../state/app";
import { selectLoopActive, telemetryStore } from "../state/telemetry";

/** Short label for a beat count: `1/4`, `1/2`, `1`, `32`. */
export function beatLabel(beats: number): string {
  if (beats === 0.25) return "1/4";
  if (beats === 0.5) return "1/2";
  return String(beats);
}

interface LoopControlsProps {
  deck: number;
  disabled: boolean;
}

/**
 * Loop IN, OUT and RELOOP/EXIT, auto loops of 1/4 to 32 beats, halve and
 * double. A size button rolls while held when ROLL is on or Shift is down.
 */
export function LoopControls({ deck, disabled }: LoopControlsProps) {
  const { rollMode } = useAppStore(selectDeckUi(deck));
  const looping = useStore(telemetryStore, (s) => selectLoopActive(s, deck));
  // A roll started on pointer down must not also fire the click that follows.
  const rolled = useRef(false);

  const onSizeDown = (event: PointerEvent<HTMLButtonElement>, beats: number) => {
    if (!rollMode && !event.shiftKey) {
      return;
    }
    rolled.current = true;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    loopRollPress(deck, beats);
  };
  const onSizeUp = () => loopRollRelease(deck);
  const onSizeClick = (beats: number) => {
    if (rolled.current) {
      rolled.current = false;
      return;
    }
    autoLoop(deck, beats);
  };

  return (
    <div className="flex flex-col gap-2" role="group" aria-label="Loop">
      <div className="flex flex-wrap items-center gap-2">
        <Button size="sm" disabled={disabled} onClick={() => loopIn(deck)}>
          IN
        </Button>
        <Button size="sm" disabled={disabled} onClick={() => loopOut(deck)}>
          OUT
        </Button>
        <Button size="sm" disabled={disabled} onClick={() => loopToggle(deck)}>
          {looping ? "EXIT" : "RELOOP"}
        </Button>
        <Button
          size="sm"
          aria-label="Halve loop"
          disabled={disabled}
          onClick={() => resizeLoop(deck, 0.5)}
        >
          1/2x
        </Button>
        <Button
          size="sm"
          aria-label="Double loop"
          disabled={disabled}
          onClick={() => resizeLoop(deck, 2)}
        >
          2x
        </Button>
        <Toggle
          variant="outline"
          size="sm"
          aria-label="Loop roll"
          pressed={rollMode}
          disabled={disabled}
          onPressedChange={(on) => setRollMode(deck, on)}
        >
          ROLL
        </Toggle>
        {looping ? <Badge variant="warning">LOOP</Badge> : null}
      </div>
      <div className="flex flex-wrap gap-1" role="group" aria-label="Auto loop beats">
        {LOOP_BEATS.map((beats) => (
          <Button
            key={beats}
            size="sm"
            className="min-w-8 px-1"
            aria-label={`${rollMode ? "Roll" : "Loop"} ${beatLabel(beats)} beats`}
            disabled={disabled}
            onPointerDown={(event) => onSizeDown(event, beats)}
            onPointerUp={onSizeUp}
            onPointerCancel={onSizeUp}
            onLostPointerCapture={onSizeUp}
            onClick={() => onSizeClick(beats)}
          >
            {beatLabel(beats)}
          </Button>
        ))}
      </div>
    </div>
  );
}
