import { Popover, PopoverContent, PopoverTrigger, Button, Toggle } from "@opendeck/design-system";
import { SlidersHorizontal } from "lucide-react";
import { useStore } from "zustand";
import { findEffect, slotRef, unitRef } from "../../lib/fx";
import { applyFx, useFxStore } from "../../state/fx";
import { telemetryStore } from "../../state/telemetry";
import { EffectSelect } from "./EffectSelect";
import { ParamKnobs } from "./ParamKnobs";
import { PresetMenu } from "./PresetMenu";

interface SendUnitProps {
  /** Bus index. */
  bus: number;
}

/** Editor of one send/return bus: up to three effects on the return, tap point. */
export function SendUnit({ bus }: SendUnitProps) {
  const send = useFxStore((s) => s.state?.sends[bus]);
  const effects = useFxStore((s) => s.effects);
  const active = useStore(telemetryStore, (s) => s.telemetry?.fx_sends[bus]?.active ?? false);
  if (!send) {
    return null;
  }
  const scope = `Send ${bus + 1}`;
  return (
    <section
      aria-label={scope}
      className="flex min-w-60 flex-1 flex-col gap-2 rounded-md border border-border bg-bg-subtle p-2"
    >
      <header className="flex items-center gap-2">
        <h3 className="text-sm font-semibold text-fg">{scope}</h3>
        <span
          role="status"
          aria-label={`${scope} return`}
          className={active ? "text-xs text-accent" : "text-xs text-fg-subtle"}
        >
          {active ? "active" : "idle"}
        </span>
        <Toggle
          variant="outline"
          size="sm"
          className="ml-auto"
          aria-label={`${scope} pre-fader`}
          pressed={!send.post_fader}
          onPressedChange={(pre) => applyFx({ kind: "set_send_tap", bus, post_fader: !pre })}
        >
          PRE
        </Toggle>
        <PresetMenu slot={slotRef("send", bus, 0)} unit={unitRef("send", bus)} scope={scope} />
      </header>
      <ul className="flex flex-col gap-1">
        {send.slots.map((slot, i) => {
          const effect = findEffect(effects, slot.effect);
          const ref = slotRef("send", bus, i);
          const slotScope = `${scope} slot ${i + 1}`;
          return (
            <li key={i} className="flex items-center gap-2">
              <Toggle
                variant="outline"
                size="sm"
                aria-label={`${slotScope} on`}
                pressed={slot.enabled}
                disabled={!slot.effect}
                onPressedChange={(on) => applyFx({ kind: "set_slot_enabled", slot: ref, on })}
              >
                {i + 1}
              </Toggle>
              <div className="min-w-0 flex-1">
                <EffectSelect
                  label={`${slotScope} effect`}
                  effects={effects}
                  value={slot.effect}
                  onChange={(next) => applyFx({ kind: "set_effect", slot: ref, effect: next })}
                />
              </div>
              {effect ? (
                <Popover>
                  <PopoverTrigger asChild>
                    <Button size="sm" variant="ghost" aria-label={`${slotScope} parameters`}>
                      <SlidersHorizontal aria-hidden />
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent align="end" className="w-80 p-3">
                    <ParamKnobs slot={slot} effect={effect} slotRef={ref} scope={slotScope} />
                  </PopoverContent>
                </Popover>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
