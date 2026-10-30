import {
  Button,
  IconButton,
  Input,
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@opendeck/design-system";
import { Trash2 } from "lucide-react";
import { useState } from "react";
import type { FxPreset, FxSlotRef, FxUnitRef } from "../../ipc/bindings";
import { sameKey } from "../../lib/fx";
import { applyFx, deleteChain, deletePreset, saveChain, savePreset, useFxStore } from "../../state/fx";

interface PresetMenuProps {
  slot: FxSlotRef;
  /** Unit whose effects the chain presets store; chains are for Beat FX and sends alike. */
  unit: FxUnitRef;
  scope: string;
}

/** Presets of the slot's effect (factory and user) and chain presets of its unit. */
export function PresetMenu({ slot, unit, scope }: PresetMenuProps) {
  const state = useFxStore((s) => s.state);
  const presets = useFxStore((s) => s.presets);
  const [presetName, setPresetName] = useState("");
  const [chainName, setChainName] = useState("");
  const current = (
    slot.unit.target === "beat" ? state?.units[slot.unit.unit] : state?.sends[slot.unit.unit]
  )?.slots[slot.slot]?.effect;
  const forEffect = (list: FxPreset[] | undefined) =>
    list?.filter((p) => current && sameKey(p.effect, current)) ?? [];
  const factory = forEffect(presets?.factory);
  const user = forEffect(presets?.user);
  const apply = (preset: FxPreset) => applyFx({ kind: "apply_preset", slot, preset });

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button size="sm" variant="ghost" aria-label={`${scope} presets`}>
          Presets
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="flex w-64 flex-col gap-3 p-3">
        <section aria-label="Effect presets" className="flex flex-col gap-1">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-fg-muted">Effect</h4>
          {!current ? <p className="text-xs text-fg-subtle">Choose an effect first.</p> : null}
          <ul className="flex flex-col gap-px">
            {factory.map((p) => (
              <li key={`f-${p.name}`}>
                <Button
                  size="sm"
                  variant="ghost"
                  className="w-full justify-start"
                  onClick={() => apply(p)}
                >
                  {p.name}
                </Button>
              </li>
            ))}
            {user.map((p) => (
              <li key={`u-${p.name}`} className="flex items-center gap-1">
                <Button
                  size="sm"
                  variant="ghost"
                  className="flex-1 justify-start"
                  onClick={() => apply(p)}
                >
                  {p.name}
                </Button>
                <IconButton
                  size="sm"
                  aria-label={`Delete preset ${p.name}`}
                  onClick={() => void deletePreset(p.effect, p.name)}
                >
                  <Trash2 aria-hidden />
                </IconButton>
              </li>
            ))}
          </ul>
          <form
            className="flex gap-1"
            onSubmit={(e) => {
              e.preventDefault();
              const name = presetName.trim();
              if (name && current) {
                void savePreset(slot, name);
                setPresetName("");
              }
            }}
          >
            <Input
              aria-label="Preset name"
              placeholder="Preset name"
              value={presetName}
              onChange={(e) => setPresetName(e.target.value)}
            />
            <Button size="sm" type="submit" disabled={!current || !presetName.trim()}>
              Save
            </Button>
          </form>
        </section>
        <section aria-label="Chain presets" className="flex flex-col gap-1">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-fg-muted">Chain</h4>
          <ul className="flex flex-col gap-px">
            {(presets?.chains ?? []).map((c) => (
              <li key={c.name} className="flex items-center gap-1">
                <Button
                  size="sm"
                  variant="ghost"
                  className="flex-1 justify-start"
                  onClick={() => applyFx({ kind: "apply_chain", unit, name: c.name })}
                >
                  {c.name}
                </Button>
                <IconButton
                  size="sm"
                  aria-label={`Delete chain ${c.name}`}
                  onClick={() => void deleteChain(c.name)}
                >
                  <Trash2 aria-hidden />
                </IconButton>
              </li>
            ))}
          </ul>
          <form
            className="flex gap-1"
            onSubmit={(e) => {
              e.preventDefault();
              const name = chainName.trim();
              if (name) {
                void saveChain(unit, name);
                setChainName("");
              }
            }}
          >
            <Input
              aria-label="Chain name"
              placeholder="Chain name"
              value={chainName}
              onChange={(e) => setChainName(e.target.value)}
            />
            <Button size="sm" type="submit" disabled={!chainName.trim()}>
              Save
            </Button>
          </form>
        </section>
      </PopoverContent>
    </Popover>
  );
}
