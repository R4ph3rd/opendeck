import type { FxEffectDto, FxSlotDto, FxSlotRef } from "../../ipc/bindings";
import { defaultNormalized, formatParam, formatWet, paramStep } from "../../lib/fx";
import { applyFx } from "../../state/fx";
import { Knob } from "../Knob";

interface ParamKnobsProps {
  slot: FxSlotDto;
  effect: FxEffectDto;
  slotRef: FxSlotRef;
  /** Prefix for accessible names ("Unit 1", "Send 2 slot 1"). */
  scope: string;
}

/** One knob per parameter of the effect, with its unit, and the wet/dry mix. */
export function ParamKnobs({ slot, effect, slotRef, scope }: ParamKnobsProps) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(5rem,1fr))] gap-2">
      {effect.params.map((desc, i) => (
        <Knob
          key={desc.id}
          label={desc.name}
          name={`${scope} ${effect.name} ${desc.name}`}
          value={slot.params[i] ?? 0}
          defaultValue={defaultNormalized(desc)}
          step={paramStep(desc)}
          format={(v) => formatParam(desc, v)}
          onChange={(value) => applyFx({ kind: "set_param", slot: slotRef, param: desc.id, value })}
        />
      ))}
      <Knob
        label="Mix"
        name={`${scope} ${effect.name} mix`}
        value={slot.wet ?? 1}
        defaultValue={1}
        format={formatWet}
        onChange={(wet) => applyFx({ kind: "set_wet", slot: slotRef, wet })}
      />
    </div>
  );
}
