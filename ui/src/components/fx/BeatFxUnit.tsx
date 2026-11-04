import {
  Button,
  Input,
  Popover,
  PopoverContent,
  PopoverTrigger,
  Toggle,
  ToggleGroup,
  ToggleGroupItem,
} from "@opendeck/design-system";
import { Settings2 } from "lucide-react";
import { useStore } from "zustand";
import type { BeatFxUnitDto, FxEffectDto, FxMode } from "../../ipc/bindings";
import {
  ASSIGN_OPTIONS,
  assignedDecks,
  defaultNormalized,
  findEffect,
  formatDivision,
  formatParam,
  slotRef,
  unitRef,
} from "../../lib/fx";
import { applyFx, useFxStore } from "../../state/fx";
import { telemetryStore } from "../../state/telemetry";
import { Knob } from "../Knob";
import { DivisionStepper } from "./DivisionStepper";
import { EffectSelect } from "./EffectSelect";
import { ParamKnobs } from "./ParamKnobs";
import { PresetMenu } from "./PresetMenu";

interface BeatFxUnitProps {
  /** Unit index, `0` or `1`. */
  index: number;
}

/** One FLX4-style Beat FX unit: channel, effects, beat, level, ON/OFF, release, BPM. */
export function BeatFxUnit({ index }: BeatFxUnitProps) {
  const unit = useFxStore((s) => s.state?.units[index]);
  const effects = useFxStore((s) => s.effects);
  const decks = unit ? assignedDecks(unit.assign) : [];
  // Lit from the engine: the chain on an assigned channel really runs.
  const running = useStore(telemetryStore, (s) =>
    decks.some((d) => s.telemetry?.fx_decks[d]?.enabled ?? false),
  );
  const hasTelemetry = useStore(telemetryStore, (s) => s.telemetry !== null);
  const ringing = useStore(telemetryStore, (s) =>
    decks.some((d) => s.telemetry?.fx_decks[d]?.active ?? false),
  );
  const trackBpm = useStore(telemetryStore, (s) => {
    for (const d of decks) {
      const bpm = s.telemetry?.decks[d]?.bpm;
      if (bpm) {
        return bpm;
      }
    }
    return null;
  });
  if (!unit) {
    return null;
  }
  const n = index + 1;
  const scope = `Beat FX ${n}`;
  const single = unit.mode === "single";
  const visible = single ? unit.slots.slice(0, 1) : unit.slots;
  const lead = unit.slots[0];
  const synced = unit.slots.some((s) => findEffect(effects, s.effect)?.beat_synced);
  const on = hasTelemetry ? running : unit.on;
  const uref = unitRef("beat", index);
  const bpmText =
    unit.bpm_source === "tap"
      ? unit.tap_bpm
        ? unit.tap_bpm.toFixed(1)
        : "tap"
      : trackBpm
        ? trackBpm.toFixed(1)
        : "-";

  return (
    <section
      aria-label={scope}
      className="flex min-w-72 flex-1 flex-col gap-2 rounded-md border border-border bg-bg-subtle p-2"
    >
      <header className="flex flex-wrap items-center gap-2">
        <h3 className="text-sm font-medium text-fg">{`FX ${n}`}</h3>
        <ToggleGroup
          type="single"
          aria-label={`${scope} channel`}
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
              aria-label={`${scope} channel ${o.label}`}
            >
              {o.label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <ToggleGroup
          type="single"
          aria-label={`${scope} mode`}
          value={unit.mode}
          onValueChange={(value) => {
            if (value) {
              applyFx({ kind: "set_mode", unit: index, mode: value as FxMode });
            }
          }}
        >
          <ToggleGroupItem value="multi" size="sm" aria-label={`${scope} multi mode`}>
            Multi
          </ToggleGroupItem>
          <ToggleGroupItem value="single" size="sm" aria-label={`${scope} single mode`}>
            Single
          </ToggleGroupItem>
        </ToggleGroup>
        <div className="ml-auto flex items-center gap-1">
          <PresetMenu slot={slotRef("beat", index, 0)} unit={uref} scope={scope} />
          <ReleaseSettings unit={unit} index={index} effects={effects} scope={scope} />
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <DivisionStepper
          unit={uref}
          division={lead?.division ?? null}
          disabled={!synced}
          scope={scope}
        />
        <Toggle
          variant="outline"
          size="sm"
          aria-label={`${scope} on/off`}
          pressed={on}
          disabled={unit.assign === "none"}
          onPressedChange={(next) => applyFx({ kind: "set_unit_on", unit: index, on: next })}
        >
          ON/OFF
        </Toggle>
        {!on && ringing ? (
          <span className="text-xs text-fg-subtle" aria-label={`${scope} tail`}>
            tail
          </span>
        ) : null}
        <Button
          size="sm"
          variant="outline"
          aria-label={`${scope} release`}
          disabled={unit.assign === "none"}
          onClick={() => applyFx({ kind: "release", unit: index })}
        >
          Release FX
        </Button>
        <div className="ml-auto flex items-center gap-1" role="group" aria-label={`${scope} BPM`}>
          <ToggleGroup
            type="single"
            aria-label={`${scope} BPM source`}
            value={unit.bpm_source}
            onValueChange={(value) => {
              if (value) {
                applyFx({
                  kind: "set_bpm_source",
                  unit: index,
                  source: value as BeatFxUnitDto["bpm_source"],
                });
              }
            }}
          >
            <ToggleGroupItem value="auto" size="sm" aria-label={`${scope} BPM auto`}>
              Auto
            </ToggleGroupItem>
            <ToggleGroupItem value="tap" size="sm" aria-label={`${scope} BPM tap mode`}>
              Tap
            </ToggleGroupItem>
          </ToggleGroup>
          <Button
            size="sm"
            variant="outline"
            aria-label={`${scope} tap tempo`}
            onClick={() => applyFx({ kind: "tap", unit: index })}
          >
            TAP
          </Button>
          <output
            aria-label={`${scope} BPM readout`}
            className="min-w-12 text-right font-mono text-xs tabular-nums text-fg"
          >
            {bpmText}
          </output>
        </div>
      </div>

      <ul className="flex flex-col gap-2">
        {visible.map((slot, i) => {
          const effect = findEffect(effects, slot.effect);
          const ref = slotRef("beat", index, i);
          const slotScope = single ? scope : `${scope} slot ${i + 1}`;
          return (
            <li key={i} className="flex flex-col gap-2">
              <div className="flex items-center gap-2">
                <div className="w-40 shrink-0">
                  <EffectSelect
                    label={`${slotScope} effect`}
                    effects={effects}
                    value={slot.effect}
                    onChange={(next) => applyFx({ kind: "set_effect", slot: ref, effect: next })}
                  />
                </div>
                {effect ? (
                  <LevelKnob
                    effect={effect}
                    value={slot.params[effect.primary ?? 0] ?? 0}
                    onChange={(value) => applyFx({ kind: "set_level", slot: ref, value })}
                    scope={slotScope}
                  />
                ) : null}
              </div>
              {single && effect ? (
                <ParamKnobs slot={slot} effect={effect} slotRef={ref} scope={scope} />
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

interface LevelKnobProps {
  effect: FxEffectDto;
  compact?: boolean;
  value: number;
  onChange: (value: number) => void;
  scope: string;
}

/** LEVEL/DEPTH: the effect's primary parameter. */
export function LevelKnob({ effect, compact, value, onChange, scope }: LevelKnobProps) {
  const desc = effect.params.find((p) => p.id === effect.primary);
  if (!desc) {
    return null;
  }
  return (
    <Knob
      compact={compact ?? false}
      label="Level"
      name={`${scope} level`}
      value={value}
      defaultValue={defaultNormalized(desc)}
      format={(v) => formatParam(desc, v)}
      onChange={onChange}
    />
  );
}

interface ReleaseSettingsProps {
  unit: BeatFxUnitDto;
  index: number;
  effects: FxEffectDto[];
  scope: string;
}

/** Release effect and its hold time. */
function ReleaseSettings({ unit, index, effects, scope }: ReleaseSettingsProps) {
  const hold = unit.release_hold_ms;
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size="sm" variant="ghost" aria-label={`${scope} release settings`}>
          <Settings2 aria-hidden />
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="flex w-60 flex-col gap-2 p-3">
        <h4 className="ds-label text-fg-muted">Release FX</h4>
        <EffectSelect
          label={`${scope} release effect`}
          effects={effects}
          value={unit.release_effect}
          placeholder="Plain off"
          onChange={(effect) =>
            applyFx({ kind: "set_release_effect", unit: index, effect, hold_ms: hold })
          }
        />
        <label className="flex items-center justify-between gap-2 text-xs text-fg-muted">
          Hold (ms)
          <Input
            type="number"
            min={50}
            max={10000}
            step={50}
            className="w-24"
            aria-label={`${scope} release hold`}
            value={hold}
            onChange={(e) =>
              applyFx({
                kind: "set_release_effect",
                unit: index,
                effect: unit.release_effect,
                hold_ms: Math.round(Number(e.target.value) || hold),
              })
            }
          />
        </label>
        <p className="text-xs text-fg-subtle">
          Division {formatDivision(unit.slots[0]?.division)}. The release effect runs for the hold
          time, then the unit is off.
        </p>
      </PopoverContent>
    </Popover>
  );
}
