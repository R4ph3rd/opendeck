import {
  ContextMenuCheckboxItem,
  ContextMenuItem,
  ContextMenuSeparator,
  SegmentedControl,
  Toggle,
} from "@opendeck/design-system";
import { open } from "@tauri-apps/plugin-dialog";
import { useState, type DragEvent, type HTMLAttributes } from "react";
import { useStore } from "zustand";
import type { SamplerSlotDto } from "../../ipc/bindings";
import { dragKind, readDrag } from "../../lib/dnd";
import { PLAY_MODES, deckSlots, sampleLength } from "../../lib/pads";
import { errorMessage } from "../../lib/errors";
import { notifyError } from "../../state/notify";
import {
  padDown,
  padUp,
  samplerClear,
  samplerLayout,
  samplerLevel,
  samplerLoadPath,
  samplerLoadTrack,
  samplerPatch,
  samplerPfl,
  usePadStore,
} from "../../state/pads";
import { telemetryStore } from "../../state/telemetry";
import { cueToken } from "../../viz/colors";
import { PerformancePads, type PadSpec } from "../../viz/PerformancePads";
import { Knob } from "../Knob";
import { LevelMeter } from "../LevelMeter";
import { RenameDialog } from "./RenameDialog";

/** Largest slot gain (`SamplerSlotPatch.gain` range `0..=2`). */
const MAX_GAIN = 2;
/** Gain change per menu step. */
const GAIN_STEP = 0.25;
/** Hot cue palette size, colours cycle through it. */
const COLOURS = 8;

/** Layout, level, PFL and meter of the sampler, shown in the tab strip while a deck is in SAMPLER mode. */
export function SamplerControls({ deck }: { deck: number }) {
  const sampler = usePadStore((s) => s.sampler);
  if (!sampler) {
    return null;
  }
  return (
    <div className="flex items-center gap-2">
      <SegmentedControl
        aria-label={`Deck ${deck + 1} sampler layout`}
        size="sm"
        value={sampler.layout}
        options={[
          { value: "split", label: "Split" },
          { value: "shared", label: "Shared" },
        ]}
        onValueChange={(value) => samplerLayout(value === "shared" ? "shared" : "split")}
      />
      <div className="w-24">
        <Knob
          compact
          label="LVL"
          name={`Deck ${deck + 1} sampler level`}
          value={sampler.level ?? 1}
          defaultValue={1}
          format={(v) => `${Math.round(v * 100)}%`}
          onChange={samplerLevel}
        />
      </div>
      <Toggle
        size="sm"
        aria-label={`Deck ${deck + 1} sampler PFL`}
        pressed={sampler.pfl}
        className="rounded-sm bg-surface-raised px-1.5 ds-label data-[state=on]:text-accent data-[state=on]:shadow-[inset_0_0_0_1px_var(--accent)]"
        onPressedChange={samplerPfl}
      >
        PFL
      </Toggle>
      <div className="h-5 w-3">
        <LevelMeter source="sampler" label={`Deck ${deck + 1} sampler level meter`} />
      </div>
    </div>
  );
}

/** Eight sampler pads: the sample name and length, lit with the slot colour once loaded. */
export function SamplerView({ deck }: { deck: number }) {
  const sampler = usePadStore((s) => s.sampler);
  const [renaming, setRenaming] = useState<SamplerSlotDto | null>(null);
  const slots = sampler ? deckSlots(sampler.layout, deck) : [];
  // One string per render so the selector result is stable between telemetry ticks.
  const playing = useStore(telemetryStore, (s) =>
    slots.map((n) => (s.telemetry?.sampler[n]?.playing ? "1" : "0")).join(""),
  );
  if (!sampler) {
    return <p className="p-2 text-sm text-fg-subtle">The sampler is loading.</p>;
  }

  const slotOf = (index: number): SamplerSlotDto | undefined => {
    const n = slots[index];
    return n === undefined ? undefined : sampler.slots[n];
  };

  const pads: PadSpec[] = slots.map((n, index) => {
    const slot = sampler.slots[n];
    if (!slot) {
      return { label: "Empty", color: null, lit: false, disabled: true };
    }
    return {
      label: slot.loading
        ? "Loading..."
        : slot.path === null
          ? "Empty"
          : slot.name || `Sample ${n + 1}`,
      sublabel: slot.loaded ? sampleLength(slot.duration_secs ?? 0) : undefined,
      ariaLabel: `Sample slot ${n + 1}${slot.loaded ? `, ${slot.name}` : " (empty)"}`,
      color: cueToken(slot.color),
      lit: slot.loaded,
      active: playing[index] === "1",
    };
  });

  const loadFile = async (slot: SamplerSlotDto) => {
    try {
      const picked = await open({ multiple: false, directory: false });
      if (typeof picked === "string") {
        samplerLoadPath(slot.slot, picked);
      }
    } catch (error) {
      notifyError(errorMessage(error));
    }
  };

  const dropProps = (index: number): HTMLAttributes<HTMLButtonElement> => ({
    onDragOver: (event: DragEvent) => {
      const kind = dragKind(event.dataTransfer);
      if (kind === "tracks" || kind === "files") {
        event.preventDefault();
        event.dataTransfer.dropEffect = "copy";
      }
    },
    onDrop: (event: DragEvent) => {
      const slot = slotOf(index);
      const payload = readDrag(event.dataTransfer);
      if (!slot) return;
      if (payload?.kind === "tracks" && payload.ids[0] !== undefined) {
        event.preventDefault();
        samplerLoadTrack(slot.slot, payload.ids[0]);
      } else if (payload?.kind === "files") {
        const file = payload.files.find((f) => f.kind === "audio");
        if (file) {
          event.preventDefault();
          samplerLoadPath(slot.slot, file.path);
        }
      }
    },
  });

  return (
    <>
      <PerformancePads
        label={`Deck ${deck + 1} sampler slots`}
        pads={pads}
        padProps={dropProps}
        onPress={(index, { shift }) => padDown(deck, "sampler", index, shift)}
        onRelease={(index) => padUp(deck, index, false)}
        renderMenu={(index) => {
          const slot = slotOf(index);
          if (!slot) return null;
          const gain = slot.gain ?? 1;
          return (
            <>
              <ContextMenuItem onSelect={() => void loadFile(slot)}>Load file...</ContextMenuItem>
              <ContextMenuItem disabled={slot.path === null} onSelect={() => setRenaming(slot)}>
                Rename...
              </ContextMenuItem>
              <ContextMenuItem
                onSelect={() => samplerPatch(slot.slot, { color: (slot.color + 1) % COLOURS })}
              >
                Next colour
              </ContextMenuItem>
              <ContextMenuSeparator />
              {PLAY_MODES.map((m) => (
                <ContextMenuCheckboxItem
                  key={m.value}
                  checked={slot.play_mode === m.value}
                  onSelect={() => samplerPatch(slot.slot, { play_mode: m.value })}
                >
                  {m.label}
                </ContextMenuCheckboxItem>
              ))}
              <ContextMenuCheckboxItem
                checked={slot.sync}
                onSelect={() => samplerPatch(slot.slot, { sync: !slot.sync })}
              >
                Sync to leader
              </ContextMenuCheckboxItem>
              <ContextMenuItem
                disabled={gain >= MAX_GAIN}
                onSelect={() =>
                  samplerPatch(slot.slot, {
                    gain: Math.min(MAX_GAIN, gain + GAIN_STEP),
                  })
                }
              >
                {`Gain up (${gain.toFixed(2)}x)`}
              </ContextMenuItem>
              <ContextMenuItem
                disabled={gain <= 0}
                onSelect={() =>
                  samplerPatch(slot.slot, {
                    gain: Math.max(0, gain - GAIN_STEP),
                  })
                }
              >
                Gain down
              </ContextMenuItem>
              <ContextMenuSeparator />
              <ContextMenuItem
                disabled={slot.path === null}
                onSelect={() => samplerClear(slot.slot)}
              >
                Clear
              </ContextMenuItem>
            </>
          );
        }}
      />
      <RenameDialog
        open={renaming !== null}
        onOpenChange={(openState) => {
          if (!openState) setRenaming(null);
        }}
        title={`Rename sample slot ${(renaming?.slot ?? 0) + 1}`}
        initial={renaming?.name ?? ""}
        onCommit={(name) => {
          if (renaming) samplerPatch(renaming.slot, { name });
        }}
      />
    </>
  );
}
