import {
  Badge,
  IconButton,
  Input,
  SegmentedControl,
  Toggle,
  cn,
} from "@opendeck/design-system";
import { X } from "lucide-react";
import { useState, type DragEvent } from "react";
import type { SamplerPlayMode, SamplerSlotDto } from "../../ipc/bindings";
import { dragKind, readDrag } from "../../lib/dnd";
import { PLAY_MODES, deckSlots, sampleLength } from "../../lib/pads";
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
import { cueToken } from "../../viz/colors";
import { Knob } from "../Knob";

/** Largest slot gain (`SamplerSlotPatch.gain` range `0..=2`). */
const MAX_GAIN = 2;

function SlotName({ slot }: { slot: SamplerSlotDto }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft !== null && draft !== slot.name) {
      samplerPatch(slot.slot, { name: draft });
    }
    setDraft(null);
  };
  return (
    <Input
      aria-label={`Slot ${slot.slot + 1} name`}
      placeholder="Empty"
      className="h-7 min-w-0 flex-1"
      value={draft ?? slot.name}
      disabled={slot.path === null}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
      }}
    />
  );
}

/** Peak bars of the sample, or a flat bar while empty or decoding. */
function MiniWave({ slot }: { slot: SamplerSlotDto }) {
  return (
    <div
      role="img"
      aria-label={`Slot ${slot.slot + 1} ${slot.loaded ? sampleLength(slot.duration_secs ?? 0) : "empty"}`}
      className="flex h-6 items-center gap-px"
    >
      {slot.peaks.length === 0 ? (
        <span className="h-px flex-1 bg-border" />
      ) : (
        slot.peaks.map((p, i) => (
          <span
            key={i}
            className="flex-1 rounded-sm"
            style={{
              height: `${Math.max(4, Math.round((p ?? 0) * 100))}%`,
              backgroundColor: `var(--${cueToken(slot.color)})`,
            }}
          />
        ))
      )}
    </div>
  );
}

function SlotCard({ deck, slot, index }: { deck: number; slot: SamplerSlotDto; index: number }) {
  const [over, setOver] = useState(false);
  const onDragOver = (event: DragEvent) => {
    const kind = dragKind(event.dataTransfer);
    if (kind === "tracks" || kind === "files") {
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
      setOver(true);
    }
  };
  const onDrop = (event: DragEvent) => {
    setOver(false);
    const payload = readDrag(event.dataTransfer);
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
  };
  const color = `var(--${cueToken(slot.color)})`;
  return (
    <li
      aria-label={`Sampler slot ${slot.slot + 1}`}
      className={cn(
        "flex min-w-0 flex-col gap-1 rounded-md border bg-bg-subtle p-1",
        over ? "border-accent" : "border-border",
      )}
      onDragOver={onDragOver}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
    >
      <div className="flex items-center gap-1">
        <button
          type="button"
          aria-label={`Play slot ${slot.slot + 1}`}
          aria-pressed={slot.loaded}
          disabled={!slot.loaded}
          style={slot.loaded ? { backgroundColor: color, color: "var(--cue-fg)" } : undefined}
          className={cn(
            "h-7 w-8 shrink-0 select-none rounded-md border border-border text-xs font-semibold tabular-nums",
            "disabled:opacity-50",
            !slot.loaded && "bg-surface-raised text-fg-muted",
          )}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture?.(event.pointerId);
            padDown(deck, "sampler", index, false);
          }}
          onPointerUp={() => padUp(deck, index, false)}
          onPointerCancel={() => padUp(deck, index, false)}
          onLostPointerCapture={() => padUp(deck, index, false)}
        >
          {slot.slot + 1}
        </button>
        <SlotName slot={slot} />
        <IconButton
          size="sm"
          aria-label={`Clear slot ${slot.slot + 1}`}
          disabled={slot.path === null}
          onClick={() => samplerClear(slot.slot)}
        >
          <X className="size-4" aria-hidden />
        </IconButton>
      </div>
      <MiniWave slot={slot} />
      <div className="flex items-center gap-1">
        <Knob
          label="Gain"
          name={`Slot ${slot.slot + 1} gain`}
          value={slot.gain ?? 1}
          min={0}
          max={MAX_GAIN}
          defaultValue={1}
          format={(v) => `${v.toFixed(2)}x`}
          onChange={(gain) => samplerPatch(slot.slot, { gain })}
        />
        <div className="flex min-w-0 flex-1 flex-col items-start gap-1">
          <SegmentedControl
            aria-label={`Slot ${slot.slot + 1} play mode`}
            size="sm"
            value={slot.play_mode}
            options={PLAY_MODES.map((m) => ({ value: m.value, label: m.label }))}
            onValueChange={(value) => samplerPatch(slot.slot, { play_mode: value as SamplerPlayMode })}
          />
          <div className="flex items-center gap-1">
            <Toggle
              variant="outline"
              size="sm"
              aria-label={`Slot ${slot.slot + 1} sync`}
              pressed={slot.sync}
              onPressedChange={(sync) => samplerPatch(slot.slot, { sync })}
            >
              Sync
            </Toggle>
            <button
              type="button"
              aria-label={`Slot ${slot.slot + 1} colour`}
              className="size-6 rounded-md border border-border"
              style={{ backgroundColor: color }}
              onClick={() => samplerPatch(slot.slot, { color: (slot.color + 1) % 8 })}
            />
            {slot.loading ? <Badge variant="neutral">Loading</Badge> : null}
          </div>
        </div>
      </div>
    </li>
  );
}

/** The eight slots a deck's pads reach, with the sampler level, PFL and slot layout. */
export function SamplerView({ deck }: { deck: number }) {
  const sampler = usePadStore((s) => s.sampler);
  if (!sampler) {
    return <p className="text-sm text-fg-subtle">The sampler is loading.</p>;
  }
  const slots = deckSlots(sampler.layout, deck);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
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
        <Knob
          label="Level"
          name={`Deck ${deck + 1} sampler level`}
          value={sampler.level ?? 1}
          defaultValue={1}
          format={(v) => `${Math.round(v * 100)}%`}
          onChange={samplerLevel}
        />
        <Toggle
          variant="outline"
          size="sm"
          aria-label={`Deck ${deck + 1} sampler PFL`}
          pressed={sampler.pfl}
          onPressedChange={samplerPfl}
        >
          PFL
        </Toggle>
      </div>
      <ul aria-label={`Deck ${deck + 1} sampler slots`} className="grid grid-cols-1 gap-1 xl:grid-cols-2">
        {slots.map((n, i) => {
          const slot = sampler.slots[n];
          return slot ? <SlotCard key={n} deck={deck} slot={slot} index={i} /> : null;
        })}
      </ul>
    </div>
  );
}
