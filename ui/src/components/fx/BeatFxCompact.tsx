import {
  Button,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Toggle,
  ToggleGroup,
  ToggleGroupItem,
} from "@opendeck/design-system";
import { SlidersHorizontal } from "lucide-react";
import { useStore } from "zustand";
import type { BeatFxUnitDto } from "../../ipc/bindings";
import { ASSIGN_OPTIONS, assignedDecks, findEffect, slotRef, unitRef } from "../../lib/fx";
import { applyFx, useFxStore } from "../../state/fx";
import { telemetryStore } from "../../state/telemetry";
import { BeatFxUnit, LevelKnob } from "./BeatFxUnit";
import { DivisionStepper } from "./DivisionStepper";
import { EffectSelect } from "./EffectSelect";

/**
 * Beat FX unit as it sits in the middle row: channel, effect, beat, level and
 * ON in one compact block. Slots 2 and 3, single mode, release and BPM source
 * are in the "More" popover, which holds the full unit.
 */
export function BeatFxCompact({ index }: { index: number }) {
  const unit = useFxStore((s) => s.state?.units[index]);
  const effects = useFxStore((s) => s.effects);
  const decks = unit ? assignedDecks(unit.assign) : [];
  const running = useStore(telemetryStore, (s) =>
    decks.some((d) => s.telemetry?.fx_decks[d]?.enabled ?? false),
  );
  const hasTelemetry = useStore(telemetryStore, (s) => s.telemetry !== null);
  if (!unit) {
    return <p className="p-2 text-sm text-fg-subtle">Effects are loading.</p>;
  }
  const n = index + 1;
  const scope = `Beat FX ${n}`;
  const quick = `${scope} quick`;
  const lead = unit.slots[0];
  const effect = findEffect(effects, lead?.effect ?? null);
  const synced = unit.slots.some((s) => findEffect(effects, s.effect)?.beat_synced);
  const on = hasTelemetry ? running : unit.on;
  const ref = slotRef("beat", index, 0);

  return (
    <section aria-label={quick} className="flex size-full min-w-0 flex-col gap-1 p-2">
      <header className="flex items-center gap-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-fg-muted">{scope}</h3>
        <ToggleGroup
          type="single"
          aria-label={`${quick} channel`}
          value={unit.assign === "none" ? "" : unit.assign}
          onValueChange={(value) => {
            if (value) {
              applyFx({ kind: "set_assign", unit: index, assign: value as BeatFxUnitDto["assign"] });
            }
          }}
        >
          {ASSIGN_OPTIONS.map((o) => (
            <ToggleGroupItem
              key={o.value}
              value={o.value}
              size="sm"
              aria-label={`${quick} channel ${o.label}`}
            >
              {o.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <Popover>
          <PopoverTrigger asChild>
            <Button size="sm" variant="ghost" className="ml-auto" aria-label={`${scope} more`}>
              <SlidersHorizontal aria-hidden />
              More
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" className="w-[var(--fx-unit-w)] min-w-[var(--fx-unit-w)] p-0">
            <BeatFxUnit index={index} />
          </PopoverContent>
        </Popover>
      </header>
      <div className="flex min-h-0 flex-1 items-center gap-2">
        <div className="min-w-0 flex-1">
          <EffectSelect
            label={`${quick} effect`}
            effects={effects}
            value={lead?.effect ?? null}
            onChange={(next) => applyFx({ kind: "set_effect", slot: ref, effect: next })}
          />
        </div>
        {effect ? (
          <div className="w-24 shrink-0">
            <LevelKnob
              compact
              effect={effect}
              value={lead?.params[effect.primary ?? 0] ?? 0}
              onChange={(value) => applyFx({ kind: "set_level", slot: ref, value })}
              scope={quick}
            />
          </div>
        ) : null}
      </div>
      <div className="flex items-center gap-2">
        <DivisionStepper
          unit={unitRef("beat", index)}
          division={lead?.division ?? null}
          disabled={!synced}
          scope={quick}
        />
        <Toggle
          variant="outline"
          size="sm"
          className="ml-auto"
          aria-label={`${quick} on/off`}
          pressed={on}
          disabled={unit.assign === "none"}
          onPressedChange={(next) => applyFx({ kind: "set_unit_on", unit: index, on: next })}
        >
          ON
        </Toggle>
      </div>
    </section>
  );
}
