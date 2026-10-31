import { useFxStore, applyFx } from "../../state/fx";
import { deckLetter } from "../../lib/deck";
import { effectKeyId, formatCfx, formatSend } from "../../lib/fx";
import { Knob } from "../Knob";
import { EffectSelect } from "./EffectSelect";

interface ChannelFxProps {
  deck: number;
}

/** Sound Color FX knob of a channel: bipolar, centre is neutral. Renders nothing until the FX model has been read. */
export function CfxKnob({ deck, compact = false }: ChannelFxProps & { compact?: boolean }) {
  const cfx = useFxStore((s) => s.state?.cfx[deck]);
  if (!cfx) {
    return null;
  }
  return (
    <Knob
      label="CFX"
      name={`Sound Color FX ${deckLetter(deck)}`}
      value={cfx.amount ?? 0}
      defaultValue={0}
      min={-1}
      max={1}
      bipolar
      compact={compact}
      format={formatCfx}
      onChange={(amount) => applyFx({ kind: "set_cfx", deck, amount })}
    />
  );
}

/** Sound Color FX type picker of a channel. */
export function CfxType({ deck }: ChannelFxProps) {
  const cfx = useFxStore((s) => s.state?.cfx[deck]);
  const effects = useFxStore((s) => s.effects);
  const kinds = useFxStore((s) => s.state?.cfx_kinds);
  if (!cfx) {
    return null;
  }
  const allowed = new Set((kinds ?? []).map(effectKeyId));
  return (
    <EffectSelect
      label={`Sound Color FX type ${deckLetter(deck)}`}
      effects={effects}
      value={cfx.kind}
      allowEmpty={false}
      only={(e) => allowed.has(effectKeyId(e.key))}
      onChange={(effect) => {
        if (effect) {
          applyFx({ kind: "set_cfx_kind", deck, effect });
        }
      }}
      className="h-6 w-full"
    />
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
    <>
      {levels.map((level, bus) => (
        <Knob
          key={bus}
          label={`S${bus + 1}`}
          name={`Send ${bus + 1} ${letter}`}
          value={level ?? 0}
          defaultValue={0}
          compact
          format={formatSend}
          onChange={(value) => applyFx({ kind: "set_send_level", deck, bus, level: value })}
        />
      ))}
    </>
  );
}
