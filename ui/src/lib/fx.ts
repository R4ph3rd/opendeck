import type {
  EffectKey,
  FxAssign,
  FxEffectDto,
  FxOp,
  FxSlotRef,
  FxUnitRef,
  ParamDescriptor,
} from "../ipc/bindings";

/** Pad FX layout: banks, shift layers, pads per layer (`app-core::fx_types`). */
export const PAD_BANKS = 2;
export const PAD_LAYERS = 2;
export const PADS_PER_LAYER = 8;
export const PAD_COUNT = PAD_BANKS * PAD_LAYERS * PADS_PER_LAYER;

/** Units the panel shows (`app-core::fx_types::BEAT_FX_UNITS`, `FX_SEND_BUSES`). */
export const BEAT_FX_UNITS_SHOWN = 2;
export const SEND_BUSES_SHOWN = 2;

/** Index into `FxStateDto.pads`. */
export function padIndex(bank: number, shift: boolean, pad: number): number {
  return bank * PAD_LAYERS * PADS_PER_LAYER + (shift ? PADS_PER_LAYER : 0) + pad;
}

/** Group title of the pads from `first`, like the hardware modes. */
export function padGroupLabel(bank: number, shift: boolean): string {
  return `Pad FX ${bank + 1}${shift ? " shift" : ""}`;
}

export const ASSIGN_OPTIONS: { value: Exclude<FxAssign, "none">; label: string }[] = [
  { value: "ch1", label: "1" },
  { value: "ch2", label: "2" },
  { value: "both", label: "1&2" },
];

/** Decks (zero-based) an assignment covers. */
export function assignedDecks(assign: FxAssign): number[] {
  switch (assign) {
    case "ch1":
      return [0];
    case "ch2":
      return [1];
    case "both":
      return [0, 1];
    default:
      return [];
  }
}

export const effectKeyId = (key: EffectKey | null): string =>
  key ? `${key.provider}:${key.id}` : "";

export const sameKey = (a: EffectKey | null, b: EffectKey | null): boolean =>
  effectKeyId(a) === effectKeyId(b);

export function unitRef(target: "beat" | "send", unit: number): FxUnitRef {
  return { target, unit };
}

export function slotRef(target: "beat" | "send", unit: number, slot: number): FxSlotRef {
  return { unit: unitRef(target, unit), slot };
}

/** Beat division as a fraction or whole number: `0.25` is "1/4", `2` is "2". */
export function formatDivision(beats: number | null | undefined): string {
  if (!beats || !Number.isFinite(beats) || beats <= 0) {
    return "-";
  }
  if (beats >= 1) {
    return Number.isInteger(beats) ? String(beats) : beats.toFixed(2);
  }
  const inverse = 1 / beats;
  if (Math.abs(inverse - Math.round(inverse)) < 1e-6) {
    return `1/${Math.round(inverse)}`;
  }
  // 3/4 and friends.
  for (const denominator of [2, 4, 8, 16]) {
    const numerator = beats * denominator;
    if (Math.abs(numerator - Math.round(numerator)) < 1e-6) {
      return `${Math.round(numerator)}/${denominator}`;
    }
  }
  return beats.toFixed(3);
}

/** Bounds of a descriptor. The generated types allow null for a non-finite float. */
function range(desc: ParamDescriptor): { min: number; max: number; def: number } {
  const min = desc.min ?? 0;
  const max = desc.max ?? min + 1;
  return { min, max, def: desc.default ?? min };
}

/** Plain value of a normalized parameter (`contracts::fx::ParamDescriptor::to_plain`). */
export function toPlain(desc: ParamDescriptor, normalized: number): number {
  const n = Math.min(1, Math.max(0, normalized));
  const { min, max } = range(desc);
  const curve = desc.curve;
  if (curve.kind === "logarithmic") {
    return min * (max / min) ** n;
  }
  if (curve.kind === "stepped") {
    const last = Math.max(2, curve.steps) - 1;
    return min + (Math.round(n * last) / last) * (max - min);
  }
  return min + n * (max - min);
}

/** Normalized position of the parameter's default. */
export function defaultNormalized(desc: ParamDescriptor): number {
  const { min, max, def } = range(desc);
  const d = Math.min(max, Math.max(min, def));
  const n =
    desc.curve.kind === "logarithmic"
      ? Math.log(d / min) / Math.log(max / min)
      : (d - min) / (max - min);
  return Math.min(1, Math.max(0, n));
}

/** Knob step: one notch per value of a stepped parameter, fine otherwise. */
export function paramStep(desc: ParamDescriptor): number {
  return desc.curve.kind === "stepped" ? 1 / (Math.max(2, desc.curve.steps) - 1) : 0.01;
}

const trim = (value: number, digits: number): string =>
  value.toFixed(digits).replace(/\.0+$|(\.\d*[1-9])0+$/, "$1");

/** Readout for a normalized parameter value: plain value with its unit. */
export function formatParam(desc: ParamDescriptor, normalized: number): string {
  const plain = toPlain(desc, normalized);
  switch (desc.unit) {
    case "percent":
      return `${Math.round(plain)}%`;
    case "decibels":
      return `${trim(plain, 1)} dB`;
    case "hertz":
      return plain >= 1000 ? `${trim(plain / 1000, 2)} kHz` : `${trim(plain, plain < 10 ? 2 : 0)} Hz`;
    case "milliseconds":
      return plain >= 1000 ? `${trim(plain / 1000, 2)} s` : `${Math.round(plain)} ms`;
    case "beats":
      return formatDivision(plain);
    case "semitones":
      return `${plain > 0 ? "+" : ""}${trim(plain, 1)} st`;
    default:
      return trim(plain, Math.abs(plain) < 10 ? 2 : 1);
  }
}

export const formatWet = (wet: number): string => `${Math.round(wet * 100)}%`;

/** Send and CFX readouts. */
export const formatSend = (level: number): string => `${Math.round(level * 100)}%`;

export function formatCfx(amount: number): string {
  const percent = Math.round(Math.abs(amount) * 100);
  if (percent === 0) {
    return "off";
  }
  return `${amount < 0 ? "L" : "R"} ${percent}%`;
}

/** Position of `division` inside the effect's list, nearest by ratio. */
export function nearestDivision(list: number[], division: number): number {
  let best = 0;
  let distance = Number.POSITIVE_INFINITY;
  list.forEach((d, i) => {
    const gap = Math.abs(Math.log(d) - Math.log(Math.max(division, 1e-6)));
    if (gap < distance) {
      distance = gap;
      best = i;
    }
  });
  return best;
}

/** Ops whose result the knob shows at once, before the backend echoes the state. */
export type LocalOp = Extract<
  FxOp,
  { kind: "set_param" | "set_level" | "set_wet" | "set_cfx" | "set_send_level" | "set_division" }
>;

/** Divisions of an effect as plain numbers. */
export function divisionsOf(effect: FxEffectDto | undefined): number[] {
  return (effect?.divisions ?? []).filter((d): d is number => typeof d === "number");
}

export function findEffect(effects: FxEffectDto[], key: EffectKey | null): FxEffectDto | undefined {
  return key ? effects.find((e) => sameKey(e.key, key)) : undefined;
}
