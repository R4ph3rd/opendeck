import {
  Panel,
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  ToggleGroup,
  ToggleGroupItem,
  cn,
  type ResizableLayout,
} from "@opendeck/design-system";
import { motion, useReducedMotion } from "motion/react";
import type { PadMode } from "../../ipc/bindings";
import { DECK_BORDER } from "../../lib/deck";
import { MODE_BUTTONS, MODE_FADE_SECONDS, MODE_LABEL, baseMode } from "../../lib/pads";
import { readStored, writeStored } from "../../lib/persist";
import { VISIBLE_DECKS, useAppStore } from "../../state/app";
import { selectPadMode } from "../../state/pads";
import { BeatJumpView } from "./BeatJumpView";
import { BeatLoopView } from "./BeatLoopView";
import { HotCueView } from "./HotCueView";
import { KeyboardView } from "./KeyboardView";
import { KeyShiftView } from "./KeyShiftView";
import { PadFxView } from "./PadFxView";
import { SamplerControls, SamplerView } from "./SamplerView";

/** Storage key of the split between the two deck pad panes. */
export const PADS_LAYOUT_KEY = "opendeck.layout.pads";

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
      className="gap-0.5 rounded-none bg-transparent p-0"
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
            className="rounded-sm bg-surface-raised px-2 text-xs font-semibold uppercase"
            onClick={(event) => selectPadMode(deck, event.shiftKey ? shifted : base)}
          >
            {here ? MODE_LABEL[active] : MODE_LABEL[base]}
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}

/** One deck's pane: a thin strip of mode tabs, then the pads filling the rest. */
function DeckPads({ deck }: { deck: number }) {
  const mode = useAppStore((s) => s.padModes[deck] ?? "hot_cue");
  const reduce = useReducedMotion();
  return (
    <section
      aria-label={`Deck ${deck + 1} pads`}
      className={cn(
        "flex size-full min-w-0 flex-col bg-bg-subtle border-t-2",
        DECK_BORDER[deck] ?? "",
      )}
    >
      <div className="flex h-8 shrink-0 items-center justify-between gap-2 px-1">
        <PadModeTabs deck={deck} />
        {mode === "sampler" ? <SamplerControls deck={deck} /> : null}
      </div>
      <motion.div
        key={mode}
        data-testid={`pad-view-${deck}`}
        data-mode={mode}
        className="min-h-0 flex-1 p-0.5 pt-0"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: reduce ? 0 : MODE_FADE_SECONDS }}
      >
        <PadModeView deck={deck} mode={mode} />
      </motion.div>
    </section>
  );
}

const DEFAULT_PADS_LAYOUT: ResizableLayout = { "pads-1": 50, "pads-2": 50 };

/** Middle band: one pad pane per deck, side by side with a draggable separator. */
export function PadMiddlePanel() {
  const stored = readStored<ResizableLayout | null>(PADS_LAYOUT_KEY, null);
  return (
    <Panel flush aria-label="Pads and effects" className="size-full">
      <ResizablePanelGroup
        orientation="horizontal"
        id="pads-layout"
        defaultLayout={stored && typeof stored === "object" ? stored : DEFAULT_PADS_LAYOUT}
        onLayoutChanged={(layout) => writeStored(PADS_LAYOUT_KEY, layout)}
      >
        {VISIBLE_DECKS.map((deck, i) => (
          <ResizablePanelPair key={deck} deck={deck} last={i === VISIBLE_DECKS.length - 1} />
        ))}
      </ResizablePanelGroup>
    </Panel>
  );
}

function ResizablePanelPair({ deck, last }: { deck: number; last: boolean }) {
  return (
    <>
      <ResizablePanel id={`pads-${deck + 1}`} minSize="20%">
        <DeckPads deck={deck} />
      </ResizablePanel>
      {last ? null : <ResizableHandle />}
    </>
  );
}
