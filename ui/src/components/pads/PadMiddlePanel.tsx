import { Panel, PanelBody, PanelHeader, ToggleGroup, ToggleGroupItem, cn } from "@opendeck/design-system";
import { motion, useReducedMotion } from "motion/react";
import type { PadMode } from "../../ipc/bindings";
import { DECK_BORDER } from "../../lib/deck";
import { MODE_BUTTONS, MODE_FADE_SECONDS, MODE_LABEL, baseMode } from "../../lib/pads";
import { VISIBLE_DECKS, useAppStore } from "../../state/app";
import { selectPadMode } from "../../state/pads";
import { BeatJumpView } from "./BeatJumpView";
import { BeatLoopView } from "./BeatLoopView";
import { HotCueView } from "./HotCueView";
import { KeyboardView } from "./KeyboardView";
import { KeyShiftView } from "./KeyShiftView";
import { PadFxView } from "./PadFxView";
import { SamplerView } from "./SamplerView";

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

/**
 * Mode buttons of a deck, mirroring the controller. A click picks the
 * button's mode, SHIFT-click its shifted partner. The label shows which of the
 * two is active.
 */
export function PadModeTabs({ deck }: { deck: number }) {
  const active = useAppStore((s) => s.padModes[deck] ?? "hot_cue");
  return (
    <ToggleGroup
      type="single"
      aria-label={`Deck ${deck + 1} pad mode`}
      value={baseMode(active)}
      onValueChange={() => undefined}
    >
      {MODE_BUTTONS.map(([base, shifted]) => {
        const here = active === base || active === shifted;
        return (
          <ToggleGroupItem
            key={base}
            value={base}
            size="sm"
            aria-label={`${MODE_LABEL[base]}, SHIFT-click for ${MODE_LABEL[shifted]}`}
            data-mode={here ? active : base}
            onClick={(event) => selectPadMode(deck, event.shiftKey ? shifted : base)}
          >
            {here ? MODE_LABEL[active] : MODE_LABEL[base]}
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}

function DeckPads({ deck }: { deck: number }) {
  const mode = useAppStore((s) => s.padModes[deck] ?? "hot_cue");
  const reduce = useReducedMotion();
  return (
    <section
      aria-label={`Deck ${deck + 1} pads`}
      className={cn("flex min-w-0 flex-1 flex-col gap-2 border-t-2 pt-2", DECK_BORDER[deck] ?? "")}
    >
      <PadModeTabs deck={deck} />
      <motion.div
        key={mode}
        data-testid={`pad-view-${deck}`}
        data-mode={mode}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: reduce ? 0 : MODE_FADE_SECONDS }}
      >
        <PadModeView deck={deck} mode={mode} />
      </motion.div>
    </section>
  );
}

/** Middle panel: one view per active pad mode, deck 1 and deck 2 side by side. */
export function PadMiddlePanel() {
  return (
    <Panel aria-label="Pads and effects" className="size-full">
      <PanelHeader title="Pads" />
      <PanelBody className="flex gap-3">
        {VISIBLE_DECKS.map((deck) => (
          <DeckPads key={deck} deck={deck} />
        ))}
      </PanelBody>
    </Panel>
  );
}
