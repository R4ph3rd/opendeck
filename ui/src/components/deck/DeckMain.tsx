import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Slider,
  cn,
} from "@opendeck/design-system";
import { ChevronLeft, ChevronRight, Pause, Play } from "lucide-react";
import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useStore } from "zustand";
import type { TempoRange } from "../../ipc/bindings";
import { formatTempo } from "../../lib/format";
import {
  autoLoop,
  beatJump,
  cuePress,
  cueRelease,
  loopIn,
  loopOut,
  loopRollPress,
  loopRollRelease,
  loopToggle,
  pitchBend,
  resizeLoop,
  setJumpBeats,
  setKeyLock,
  setRollMode,
  setTempoFader,
  setTempoRange,
  toggleQuantize,
  toggleSlip,
  togglePlay,
} from "../../state/actions";
import {
  JUMP_BEATS,
  LOOP_BEATS,
  TEMPO_RANGES,
  TEMPO_RANGE_FRACTION,
  selectDeckUi,
  useAppStore,
} from "../../state/app";
import { selectLoopActive, telemetryStore } from "../../state/telemetry";
import { DeckJog, VinylToggle } from "../../viz/DeckJog";
import { HotCueView } from "../pads/HotCueView";
import { PadModeTabs } from "../pads/PadMiddlePanel";
import { beatLabel } from "../../lib/beatLabel";
import { DeckButton, DeckToggle, RingButton } from "./DeckButton";

/** Slider resolution: fine enough for 0.1% steps on the narrowest range. */
const FADER_STEP = 0.001;

/** Label of a range in the selector: `±6`. */
const rangeLabel = (range: TempoRange): string =>
  `±${Math.round(TEMPO_RANGE_FRACTION[range] * 100)}`;

/** Wraps a control so it fits a flex cell exactly; `cq` units give the round buttons a size. */
const CELL = "flex min-h-0 flex-1 items-center justify-center [container-type:size]";

interface Props {
  deck: number;
  ready: boolean;
}

/** Beat loop size display with halve/double, AUTO/MANUAL, IN/OUT and roll. */
function LoopColumn({ deck, ready }: Props) {
  const { rollMode } = useAppStore(selectDeckUi(deck));
  const looping = useStore(telemetryStore, (s) => selectLoopActive(s, deck));
  const [index, setIndex] = useState(LOOP_BEATS.indexOf(4));
  const [manual, setManual] = useState(false);
  const rolled = useRef(false);
  const beats = LOOP_BEATS[index] ?? 4;

  const step = (direction: -1 | 1) => {
    setIndex((i) => Math.min(LOOP_BEATS.length - 1, Math.max(0, i + direction)));
    if (looping) {
      resizeLoop(deck, direction < 0 ? 0.5 : 2);
    }
  };
  const onDown = (event: PointerEvent<HTMLButtonElement>) => {
    if (manual || (!rollMode && !event.shiftKey)) return;
    rolled.current = true;
    event.currentTarget.setPointerCapture?.(event.pointerId);
    loopRollPress(deck, beats);
  };
  const onClick = () => {
    if (rolled.current) {
      rolled.current = false;
      return;
    }
    if (manual || looping) {
      loopToggle(deck);
    } else {
      autoLoop(deck, beats);
    }
  };

  return (
    <div className="flex shrink-0 items-center gap-1" role="group" aria-label="Loop">
      <DeckButton
        className={cn(
          "h-6 w-10 font-mono text-base tabular-nums normal-case",
          looping && "text-accent shadow-[inset_0_0_0_1px_var(--accent)]",
        )}
        disabled={!ready}
        aria-label={
          manual
            ? looping
              ? "EXIT"
              : "RELOOP"
            : `${rollMode ? "Roll" : "Loop"} ${beatLabel(beats)} beats`
        }
        onPointerDown={onDown}
        onPointerUp={() => loopRollRelease(deck)}
        onPointerCancel={() => loopRollRelease(deck)}
        onLostPointerCapture={() => loopRollRelease(deck)}
        onClick={onClick}
      >
        {manual ? (looping ? "EXIT" : "RELOOP") : beatLabel(beats)}
      </DeckButton>
      <div className="flex gap-0.5">
        <DeckButton
          className="h-6 w-6 px-0"
          aria-label="Halve loop"
          disabled={!ready}
          onClick={() => step(-1)}
        >
          <ChevronLeft className="size-4" aria-hidden />
        </DeckButton>
        <DeckButton
          className="h-6 w-6 px-0"
          aria-label="Double loop"
          disabled={!ready}
          onClick={() => step(1)}
        >
          <ChevronRight className="size-4" aria-hidden />
        </DeckButton>
      </div>
      {manual ? (
        <div className="flex gap-0.5">
          <DeckButton disabled={!ready} onClick={() => loopIn(deck)}>
            IN
          </DeckButton>
          <DeckButton disabled={!ready} onClick={() => loopOut(deck)}>
            OUT
          </DeckButton>
        </div>
      ) : (
        <DeckToggle
          aria-label="Loop roll"
          disabled={!ready}
          pressed={rollMode}
          onPressedChange={(on) => setRollMode(deck, on)}
        >
          ROLL
        </DeckToggle>
      )}
      <div className="flex gap-0.5">
        <DeckToggle
          aria-label="Auto loop"
          pressed={!manual}
          onPressedChange={() => setManual(false)}
        >
          AU
        </DeckToggle>
        <DeckToggle
          aria-label="Manual loop"
          pressed={manual}
          onPressedChange={() => setManual(true)}
        >
          MA
        </DeckToggle>
      </div>
    </div>
  );
}

/** CUE (orange ring) and PLAY (green ring). */
function TransportColumn({ deck, ready }: Props) {
  const playing = useStore(telemetryStore, (s) => s.telemetry?.decks[deck]?.playing ?? false);
  const onCueDown = (event: PointerEvent<HTMLButtonElement>) => {
    event.currentTarget.setPointerCapture?.(event.pointerId);
    cuePress(deck);
  };
  const onCueUp = (event: PointerEvent<HTMLButtonElement>) => {
    cueRelease(deck);
    // Keep Space free for the global play shortcut after a mouse press.
    event.currentTarget.blur();
  };
  const onCueKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    if ((event.key === " " || event.key === "Enter") && !event.repeat) {
      event.preventDefault();
      cuePress(deck);
    }
  };
  return (
    <div className="flex w-[var(--transport-w)] shrink-0 flex-col gap-1">
      <div className={CELL}>
        <RingButton
          ring="cue"
          disabled={!ready}
          aria-label="Cue"
          onPointerDown={onCueDown}
          onPointerUp={onCueUp}
          onPointerCancel={() => cueRelease(deck)}
          onLostPointerCapture={() => cueRelease(deck)}
          onKeyDown={onCueKey}
          onKeyUp={() => cueRelease(deck)}
        >
          CUE
        </RingButton>
      </div>
      <div className={CELL}>
        <RingButton
          ring="play"
          disabled={!ready}
          aria-label={playing ? "PAUSE" : "PLAY"}
          onClick={(event) => {
            togglePlay(deck);
            // A mouse click (detail > 0) hands Space back to the global shortcut.
            if (event.detail > 0) event.currentTarget.blur();
          }}
        >
          {playing ? (
            <Pause className="size-6" aria-hidden />
          ) : (
            <Play className="size-6" aria-hidden />
          )}
        </RingButton>
      </div>
    </div>
  );
}

/** SLIP, Q, MT (key lock) and VINYL, spread over the height of the wheel. */
function ModeColumn({ deck, ready }: Props) {
  const { slip, quantize, keyLock } = useAppStore(selectDeckUi(deck));
  return (
    <div className="flex w-[var(--modes-w)] shrink-0 flex-col justify-between gap-1">
      <DeckToggle
        aria-label="Slip"
        disabled={!ready}
        pressed={slip}
        onPressedChange={() => toggleSlip(deck)}
      >
        SLIP
      </DeckToggle>
      <DeckToggle
        tone="cyan"
        aria-label="Quantize"
        disabled={!ready}
        pressed={quantize}
        onPressedChange={() => toggleQuantize(deck)}
      >
        Q
      </DeckToggle>
      <DeckToggle
        aria-label="KEY LOCK"
        title="Master tempo (key lock)"
        disabled={!ready}
        pressed={keyLock}
        onPressedChange={(on) => setKeyLock(deck, on)}
      >
        MT
      </DeckToggle>
      <VinylToggle deck={deck} className="h-6" />
    </div>
  );
}

/** Beat jump size and the back and forward buttons. */
function JumpColumn({ deck, ready }: Props) {
  const { jumpBeats } = useAppStore(selectDeckUi(deck));
  return (
    <div className="flex shrink-0 items-center gap-1" role="group" aria-label="Beat jump">
      <Select
        value={String(jumpBeats)}
        disabled={!ready}
        onValueChange={(value) => setJumpBeats(deck, Number(value))}
      >
        <SelectTrigger
          className="h-6 w-14 gap-0 rounded-sm border-0 bg-surface-raised px-1.5 font-mono text-base"
          aria-label="Beat jump size"
        >
          <SelectValue>{beatLabel(jumpBeats)}</SelectValue>
        </SelectTrigger>
        <SelectContent>
          {JUMP_BEATS.map((beats) => (
            <SelectItem key={beats} value={String(beats)}>
              {beatLabel(beats)} beats
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <div className="flex gap-0.5">
        <DeckButton
          className="h-6 w-6 px-0"
          aria-label="Beat jump back"
          disabled={!ready}
          onClick={() => beatJump(deck, -1)}
        >
          <ChevronLeft className="size-4" aria-hidden />
        </DeckButton>
        <DeckButton
          className="h-6 w-6 px-0"
          aria-label="Beat jump forward"
          disabled={!ready}
          onClick={() => beatJump(deck, 1)}
        >
          <ChevronRight className="size-4" aria-hidden />
        </DeckButton>
      </div>
    </div>
  );
}

/** Tall tempo fader with the range selector and pitch bend buttons. */
function TempoColumn({ deck, ready }: Props) {
  const { tempoFader, tempoRange } = useAppStore(selectDeckUi(deck));
  const bend = (amount: number) => ({
    onPointerDown: (event: PointerEvent<HTMLButtonElement>) => {
      event.currentTarget.setPointerCapture?.(event.pointerId);
      pitchBend(deck, amount);
    },
    onPointerUp: () => pitchBend(deck, 0),
    onPointerCancel: () => pitchBend(deck, 0),
  });
  return (
    <div className="flex w-[var(--tempo-w)] shrink-0 flex-col items-center gap-1" aria-label="Tempo">
      <output aria-label="Tempo change" className="sr-only" data-testid={`tempo-readout-${deck}`}>
        {formatTempo(tempoFader, TEMPO_RANGE_FRACTION[tempoRange])}
      </output>
      <Select
        value={tempoRange}
        disabled={!ready}
        onValueChange={(value) => setTempoRange(deck, value as TempoRange)}
      >
        <SelectTrigger
          className="h-6 gap-0 rounded-sm border-0 bg-surface-raised px-1 font-mono text-xs"
          aria-label="Tempo range"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {TEMPO_RANGES.map((range) => (
            <SelectItem key={range} value={range} aria-label={`Range ${rangeLabel(range)}`}>
              {rangeLabel(range)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {/* The design-system Slider names its thumb, the group names the control for the tests and the tree. */}
      <div role="group" aria-label="Tempo fader" className="flex min-h-0 flex-1 justify-center">
        <Slider
          orientation="vertical"
          trackLength="100%"
          min={-1}
          max={1}
          step={FADER_STEP}
          detent="center"
          value={[tempoFader]}
          disabled={!ready}
          onValueChange={([value]) => setTempoFader(deck, value ?? 0)}
          onDoubleClick={() => setTempoFader(deck, 0)}
        />
      </div>
      <div className="flex w-full gap-px">
        <DeckButton
          className="h-6 flex-1 px-0"
          disabled={!ready}
          aria-label="Pitch bend down"
          {...bend(-1)}
        >
          -
        </DeckButton>
        <DeckButton
          className="h-6 flex-1 px-0"
          disabled={!ready}
          aria-label="Pitch bend up"
          {...bend(1)}
        >
          +
        </DeckButton>
      </div>
    </div>
  );
}

/**
 * The deck's working row, outer edge to jog: tempo fader, the hot cue pad
 * grid with beat loop and beat jump under it, CUE and PLAY, the jog wheel and
 * SLIP, Q, MT. Deck 2 mirrors deck 1 so both jogs sit at the mixer side.
 */
export function DeckMain({ deck, ready }: Props) {
  return (
    <div
      className={cn(
        "flex min-h-0 flex-1 items-stretch gap-2 p-2",
        deck % 2 === 1 && "flex-row-reverse",
      )}
    >
      <TempoColumn deck={deck} ready={ready} />
      <div className="flex min-w-[calc(var(--pad-min-w)*4)] flex-1 flex-col gap-1">
        <div className="flex h-[var(--deck-tabs-h)] shrink-0 items-center">
          <PadModeTabs deck={deck} />
        </div>
        <div className="min-h-[var(--pad-min-w)] min-w-0 flex-1">
          <HotCueView deck={deck} />
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1">
          <LoopColumn deck={deck} ready={ready} />
          <JumpColumn deck={deck} ready={ready} />
        </div>
      </div>
      <TransportColumn deck={deck} ready={ready} />
      <div className="flex min-w-0 max-w-[var(--jog-max)] flex-[0_1_var(--jog-max)] items-center justify-center [container-type:size]">
        <div className="size-[min(100cqh,100cqw)]">
          <DeckJog deck={deck} disabled={!ready} />
        </div>
      </div>
      <ModeColumn deck={deck} ready={ready} />
    </div>
  );
}
