import { Combobox } from "@opendeck/design-system";
import type { EffectKey, FxEffectDto } from "../../ipc/bindings";
import { effectKeyId } from "../../lib/fx";

interface EffectSelectProps {
  effects: FxEffectDto[];
  value: EffectKey | null;
  onChange: (effect: EffectKey | null) => void;
  /** Accessible name, e.g. "Effect, unit 1 slot 1". */
  label: string;
  /** Narrows the list (CFX types, release effects). */
  only?: (effect: FxEffectDto) => boolean;
  /** Shown when nothing is selected. */
  placeholder?: string;
  /** Selecting the current effect again empties the slot. */
  allowEmpty?: boolean;
  className?: string;
}

/** Searchable effect picker. */
export function EffectSelect({
  effects,
  value,
  onChange,
  label,
  only,
  placeholder = "No effect",
  allowEmpty = true,
  className = "w-full",
}: EffectSelectProps) {
  const list = only ? effects.filter(only) : effects;
  return (
    <Combobox
      aria-label={label}
      className={className}
      placeholder={placeholder}
      searchPlaceholder="Search effects"
      options={list.map((e) => ({ value: effectKeyId(e.key), label: e.name }))}
      value={effectKeyId(value)}
      onValueChange={(id) => {
        if (id === undefined) {
          if (allowEmpty) {
            onChange(null);
          }
          return;
        }
        const next = list.find((e) => effectKeyId(e.key) === id);
        if (next) {
          onChange(next.key);
        }
      }}
    />
  );
}
