import { useFxStore, applyFx } from "../../state/fx";
import { deckLetter } from "../../lib/deck";
import { effectKeyId, formatCfx, formatSend } from "../../lib/fx";
import { Knob } from "../Knob";
import { EffectSelect } from "./EffectSelect";

interface ChannelFxProps {
  deck: number;
}

/**
 * Sound Color FX of a channel: one bipolar knob, centre is neutral, with the
 * effect type. Renders nothing until the FX model has been read.
 */
export function CfxControl({ deck }: ChannelFxProps) {
  const cfx = useFxStore((s) => s.state?.cfx[deck]);
  const effects = useFxStore((s) => s.effects);
  const kinds = useFxStore((s) => s.state?.cfx_kinds);
  if (!cfx) {
    return null;
  }
  const letter = deckLetter(deck);
  const allowed = new Set((kinds ?? []).map(effectKeyId));
  return (
    <div className="flex w-full flex-col items-center gap-1">
      <Knob
        label="CFX"
        name={`Sound Color FX ${letter}`}
        value={cfx.amount ?? 0}
        defaultValue={0}
        min={-1}
        max={1}
        bipolar
        format={formatCfx}
        onChange={(amount) => applyFx({ kind: "set_cfx", deck, amount })}
      />
      <EffectSelect
        label={`Sound Color FX type ${letter}`}
        effects={effects}
        value={cfx.kind}
        allowEmpty={false}
        only={(e) => allowed.has(effectKeyId(e.key))}
        onChange={(effect) => {
          if (effect) {
            applyFx({ kind: "set_cfx_kind", deck, effect });
          }
        }}
        className="w-full"
      />
    </div>
  );
}

/** Send knobs of a channel, one per send bus. */
export function SendKnobs({ deck }: ChannelFxProps) {
  const levels = useFxStore((s) => s.state?.send_levels[deck]);
  if (!levels) {
    return null;
  }
  const letter = deckLetter(deck);
  return (
    <div className="flex w-full justify-center gap-2">
      {levels.map((level, bus) => (
        <Knob
          key={bus}
          label={`Send ${bus + 1}`}
          name={`Send ${bus + 1} ${letter}`}
          value={level ?? 0}
          defaultValue={0}
          format={formatSend}
          onChange={(value) => applyFx({ kind: "set_send_level", deck, bus, level: value })}
        />
      ))}
    </div>
  );
}
