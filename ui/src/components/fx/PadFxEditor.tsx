import { cn, Toggle, ToggleGroup, ToggleGroupItem } from "@opendeck/design-system";
import { useState } from "react";
import type { EffectKey } from "../../ipc/bindings";
import {
  PADS_PER_LAYER,
  PAD_BANKS,
  divisionsOf,
  findEffect,
  formatDivision,
  nearestDivision,
  padGroupLabel,
  padIndex,
} from "../../lib/fx";
import { applyFx, useFxStore } from "../../state/fx";
import { EffectSelect } from "./EffectSelect";

/** Pad groups in hardware order: Pad FX 1, Pad FX 1 shift, Pad FX 2, Pad FX 2 shift. */
const GROUPS = Array.from({ length: PAD_BANKS * 2 }, (_, g) => ({
  bank: Math.floor(g / 2),
  shift: g % 2 === 1,
}));

/**
 * Editor of the 32 pad effects: pick a pad, then its effect, beat division and
 * whether it fires Release FX. The M7 middle panel reuses it.
 */
export function PadFxEditor() {
  const pads = useFxStore((s) => s.state?.pads);
  const effects = useFxStore((s) => s.effects);
  const [selected, setSelected] = useState(0);
  if (!pads) {
    return <p className="text-sm text-fg-subtle">FX state is loading.</p>;
  }
  const pad = pads[selected];
  const effect = findEffect(effects, pad?.effect ?? null);
  const divisions = divisionsOf(effect);
  const nearest =
    pad && divisions.length > 0 ? divisions[nearestDivision(divisions, pad.division ?? 1)] : undefined;

  const assign = (patch: { effect?: EffectKey | null; division?: number; release?: boolean }) => {
    if (!pad) {
      return;
    }
    applyFx({
      kind: "set_pad",
      index: selected,
      pad: {
        effect: patch.effect === undefined ? pad.effect : patch.effect,
        division: patch.division ?? pad.division ?? 1,
        release: patch.release ?? pad.release,
      },
    });
  };

  return (
    <div className="flex flex-col gap-3">
      {GROUPS.map(({ bank, shift }) => (
        <section
          key={`${bank}-${shift}`}
          aria-label={padGroupLabel(bank, shift)}
          className="flex flex-col gap-1"
        >
          <h4 className="text-xs font-semibold uppercase tracking-wide text-fg-muted">
            {padGroupLabel(bank, shift)}
          </h4>
          <div className="grid grid-cols-4 gap-1 sm:grid-cols-8">
            {Array.from({ length: PADS_PER_LAYER }, (_, p) => {
              const index = padIndex(bank, shift, p);
              const assigned = pads[index];
              const e = findEffect(effects, assigned?.effect ?? null);
              return (
                <button
                  key={index}
                  type="button"
                  aria-label={`Pad ${index + 1}${e ? `, ${e.name}` : ", empty"}`}
                  aria-pressed={index === selected}
                  onClick={() => setSelected(index)}
                  className={cn(
                    "flex h-12 flex-col items-start justify-between rounded-md border px-1 py-1 text-left text-xs",
                    "hover:bg-surface-hover",
                    index === selected
                      ? "border-accent bg-accent-muted text-fg"
                      : "border-border bg-bg-subtle text-fg-muted",
                  )}
                >
                  <span className="w-full truncate font-medium">{e?.name ?? "Empty"}</span>
                  <span className="font-mono text-fg-subtle">
                    {assigned?.release ? "REL" : e?.beat_synced ? formatDivision(assigned?.division) : ""}
                  </span>
                </button>
              );
            })}
          </div>
        </section>
      ))}

      <section
        aria-label={`Pad ${selected + 1} settings`}
        className="flex flex-wrap items-center gap-3 rounded-md border border-border bg-bg-subtle p-2"
      >
        <h4 className="text-sm font-semibold text-fg">{`Pad ${selected + 1}`}</h4>
        <div className="w-48">
          <EffectSelect
            label={`Pad ${selected + 1} effect`}
            effects={effects}
            value={pad?.effect ?? null}
            onChange={(next) => {
              const first = next ? divisionsOf(findEffect(effects, next)) : undefined;
              assign({
                effect: next,
                ...(first && first.length > 0 && pad
                  ? { division: first[nearestDivision(first, pad.division ?? 1)] ?? 1 }
                  : {}),
              });
            }}
          />
        </div>
        {divisions.length > 0 ? (
          <ToggleGroup
            type="single"
            aria-label={`Pad ${selected + 1} division`}
            value={String(nearest ?? "")}
            onValueChange={(value) => {
              if (value) {
                assign({ division: Number(value) });
              }
            }}
          >
            {divisions.map((d) => (
              <ToggleGroupItem
                key={d}
                value={String(d)}
                size="sm"
                aria-label={`Pad ${selected + 1} division ${formatDivision(d)}`}
              >
                {formatDivision(d)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        ) : null}
        <Toggle
          variant="outline"
          size="sm"
          aria-label={`Pad ${selected + 1} release FX`}
          pressed={pad?.release ?? false}
          onPressedChange={(release) => assign({ release })}
        >
          Release FX
        </Toggle>
      </section>
    </div>
  );
}
