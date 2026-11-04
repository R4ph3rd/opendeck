import { Panel, ToggleGroup, ToggleGroupItem } from "@opendeck/design-system";
import { motion, useReducedMotion } from "motion/react";
import type { PadMode } from "../../ipc/bindings";
import { MODE_BUTTONS, MODE_FADE_SECONDS, MODE_LABEL, baseMode } from "../../lib/pads";
import { VISIBLE_DECKS, useAppStore } from "../../state/app";
import { selectPadMode } from "../../state/pads";
import { BeatJumpView } from "./BeatJumpView";
import { BeatLoopView } from "./BeatLoopView";
import { HotCueView } from "./HotCueView";
import { KeyboardView } from "./KeyboardView";
import { KeyShiftView } from "./KeyShiftView";
import { PadFxView } from "./PadFxView";
import { SamplerControls, SamplerView } from "./SamplerView";

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
            className="h-6 flex-1 rounded-sm bg-surface-raised px-1 ds-label leading-none data-[state=on]:bg-surface-raised data-[state=on]:text-accent data-[state=on]:shadow-[inset_0_0_0_1px_var(--accent)]"
            onClick={(event) => selectPadMode(deck, event.shiftKey ? shifted : base)}
          >
            {here ? MODE_LABEL[active] : MODE_LABEL[base]}
          </ToggleGroupItem>
        );
      })}
    </ToggleGroup>
  );
}

/** Label of the view a deck pane shows, for its header. */
const paneTitle = (deck: number, mode: PadMode): string => `Deck ${deck + 1} ${MODE_LABEL[mode]}`;

/**
 * One deck's pad-mode pane in the middle row: a thin header with the mode
 * name, then the pads filling the rest.
 */
function DeckPads({ deck, mode }: { deck: number; mode: PadMode }) {
  const reduce = useReducedMotion();
  return (
    <section
      aria-label={`Deck ${deck + 1} pads`}
      className="flex size-full min-w-0 flex-col bg-bg-subtle"
    >
      <div className="flex h-[var(--deck-tabs-h)] shrink-0 items-center justify-between gap-2 px-2">
        <h3 className="truncate ds-label text-fg-muted">
          {paneTitle(deck, mode)}
        </h3>
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

/**
 * Centre of the middle row. A deck in HOT CUE mode keeps its cues in the deck
 * panel, so its pane here shows the sampler (when SAMPLER is on); any other
 * pad mode shows its view. Panes sit side by side, deck 1 then deck 2.
 */
export function PadMiddlePanel() {
  const modes = useAppStore((s) => s.padModes);
  const samplerOpen = useAppStore((s) => s.samplerOpen);
  const panes = VISIBLE_DECKS.flatMap((deck) => {
    const mode = modes[deck] ?? "hot_cue";
    const shown: PadMode | null = mode === "hot_cue" ? (samplerOpen ? "sampler" : null) : mode;
    return shown === null ? [] : [{ deck, mode: shown }];
  });
  return (
    <Panel flush aria-label="Pads and effects" className="size-full min-w-0">
      {panes.length === 0 ? (
        <p className="p-3 text-sm text-fg-subtle">
          Turn on SAMPLER or pick a pad mode on a deck to show pads here.
        </p>
      ) : (
        <div className="flex size-full min-h-0 gap-px bg-border">
          {panes.map(({ deck, mode }) => (
            <div key={deck} className="min-w-0 flex-1">
              <DeckPads deck={deck} mode={mode} />
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}
