import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@opendeck/design-system";
import { motion, useReducedMotion } from "motion/react";
import type { PadMode } from "../../ipc/bindings";
import { MODE_FADE_SECONDS, MODE_LABEL } from "../../lib/pads";
import { useAppStore } from "../../state/app";
import { selectPadMode } from "../../state/pads";
import { BeatJumpView } from "./BeatJumpView";
import { BeatLoopView } from "./BeatLoopView";
import { HotCueView } from "./HotCueView";
import { KeyboardView } from "./KeyboardView";
import { KeyShiftView } from "./KeyShiftView";
import { PadFxView } from "./PadFxView";
import { SamplerControls, SamplerView } from "./SamplerView";

/** Every pad mode in menu order: the four controller buttons, then their SHIFT partners. */
const MODE_ORDER: readonly PadMode[] = [
  "hot_cue",
  "pad_fx1",
  "beat_jump",
  "sampler",
  "keyboard",
  "pad_fx2",
  "beat_loop",
  "key_shift",
];

/** The view of one pad mode for one deck. */
export function PadModeView({ deck, mode }: { deck: number; mode: PadMode }) {
  switch (mode) {
    case "hot_cue":
      return <HotCueView deck={deck} />;
    case "pad_fx1":
    case "pad_fx2":
      return <PadFxView deck={deck} mode={mode} />;
    case "beat_jump":
      return <BeatJumpView deck={deck} />;
    case "sampler":
      return <SamplerView deck={deck} />;
    case "keyboard":
      return <KeyboardView deck={deck} />;
    case "beat_loop":
      return <BeatLoopView deck={deck} />;
    case "key_shift":
      return <KeyShiftView deck={deck} />;
  }
}

/** Pad mode dropdown of a deck ("HOT CUE"): every mode, SHIFT partners included. */
export function PadModeSelect({ deck }: { deck: number }) {
  const active = useAppStore((s) => s.padModes[deck] ?? "hot_cue");
  return (
    <Select
      value={active}
      onValueChange={(mode) => selectPadMode(deck, mode as PadMode)}
    >
      <SelectTrigger
        className="h-6 w-28 min-w-0 shrink rounded-sm border-0 bg-surface-raised px-1.5 ds-label"
        aria-label={`Deck ${deck + 1} pad mode`}
      >
        <SelectValue>{MODE_LABEL[active].toUpperCase()}</SelectValue>
      </SelectTrigger>
      <SelectContent>
        {MODE_ORDER.map((mode) => (
          <SelectItem key={mode} value={mode} aria-label={MODE_LABEL[mode]}>
            {MODE_LABEL[mode]}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

/**
 * One deck's pad panel: the pads of the active mode in two columns of four
 * (A-D left, E-H right), the mode dropdown under them, and the sampler
 * controls beside it while the deck is in SAMPLER mode.
 */
export function DeckPads({ deck }: { deck: number }) {
  const reduce = useReducedMotion();
  const mode = useAppStore((s) => s.padModes[deck] ?? "hot_cue");
  return (
    <section
      aria-label={`Deck ${deck + 1} pads`}
      className="flex size-full min-h-0 min-w-0 flex-col gap-1"
    >
      <motion.div
        key={mode}
        data-testid={`pad-view-${deck}`}
        data-mode={mode}
        className="min-h-0 flex-1"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: reduce ? 0 : MODE_FADE_SECONDS }}
      >
        <PadModeView deck={deck} mode={mode} />
      </motion.div>
      <div className="flex shrink-0 flex-wrap items-center gap-x-2 gap-y-1">
        <PadModeSelect deck={deck} />
        {mode === "sampler" ? <SamplerControls deck={deck} /> : null}
      </div>
    </section>
  );
}
