import { create } from "zustand";
import {
  commands,
  type EffectKey,
  type FxEffectDto,
  type FxOp,
  type FxPresetsDto,
  type FxSlotDto,
  type FxSlotRef,
  type FxStateDto,
  type FxUnitRef,
} from "../ipc/bindings";
import { findEffect } from "../lib/fx";
import { report } from "./actions";

interface FxStore {
  /** Every effect the backend offers. */
  effects: FxEffectDto[];
  /** The FX model; `null` until the first read. */
  state: FxStateDto | null;
  presets: FxPresetsDto | null;
  setState: (state: FxStateDto) => void;
  setEffects: (effects: FxEffectDto[]) => void;
  setPresets: (presets: FxPresetsDto) => void;
  reset: () => void;
}

export const useFxStore = create<FxStore>()((set) => ({
  effects: [],
  state: null,
  presets: null,
  setState: (state) => set({ state }),
  setEffects: (effects) => set({ effects }),
  setPresets: (presets) => set({ presets }),
  reset: () => set({ effects: [], state: null, presets: null }),
}));

const store = () => useFxStore.getState();

function slotOf(state: FxStateDto, ref: FxSlotRef): FxSlotDto | undefined {
  const unit = ref.unit.target === "beat" ? state.units[ref.unit.unit] : state.sends[ref.unit.unit];
  return unit?.slots[ref.slot];
}

/**
 * The state a knob drag shows before the backend echoes it. Only the ops a
 * knob sends continuously are applied; everything else waits for the
 * `fx-state-changed` event.
 */
export function patchLocal(state: FxStateDto, op: FxOp, effects: FxEffectDto[]): FxStateDto {
  switch (op.kind) {
    case "set_param":
    case "set_level":
    case "set_wet":
    case "set_division": {
      const next = structuredClone(state);
      const slot = slotOf(next, op.slot);
      if (!slot) {
        return state;
      }
      if (op.kind === "set_wet") {
        slot.wet = op.wet;
      } else if (op.kind === "set_division") {
        slot.division = op.beats;
      } else {
        const param =
          op.kind === "set_param" ? op.param : findEffect(effects, slot.effect)?.primary;
        if (param === null || param === undefined || param >= slot.params.length) {
          return state;
        }
        slot.params[param] = op.value;
      }
      return next;
    }
    case "set_cfx": {
      const next = structuredClone(state);
      const cfx = next.cfx[op.deck];
      if (!cfx) {
        return state;
      }
      cfx.amount = op.amount;
      return next;
    }
    case "set_send_level": {
      const next = structuredClone(state);
      const row = next.send_levels[op.deck];
      if (!row) {
        return state;
      }
      row[op.bus] = op.level;
      return next;
    }
    default:
      return state;
  }
}

/** Sends one change to the backend; knobs update at once, the echo corrects. */
export function applyFx(op: FxOp): void {
  const { state, effects, setState } = store();
  if (state) {
    const next = patchLocal(state, op, effects);
    if (next !== state) {
      setState(next);
    }
  }
  void report(commands.fxApply(op));
}

/** One pad edge for `deck`. */
export function firePad(
  deck: number,
  bank: number,
  shift: boolean,
  pad: number,
  pressed: boolean,
): void {
  void report(commands.fxPad(deck, bank, shift, pad, pressed));
}

/** Reads effects, model and presets (startup). */
export async function hydrateFx(): Promise<void> {
  const [effects, state, presets] = await Promise.all([
    report(commands.fxListEffects()),
    report(commands.fxGetState()),
    report(commands.fxGetPresets()),
  ]);
  if (effects) {
    store().setEffects(effects);
  }
  if (state) {
    store().setState(state);
  }
  if (presets) {
    store().setPresets(presets);
  }
}

async function updatePresets(pending: ReturnType<typeof commands.fxGetPresets>): Promise<void> {
  const presets = await report(pending);
  if (presets) {
    store().setPresets(presets);
  }
}

export const savePreset = (slot: FxSlotRef, name: string) =>
  updatePresets(commands.fxSavePreset(slot, name));

export const deletePreset = (effect: EffectKey, name: string) =>
  updatePresets(commands.fxDeletePreset(effect, name));

export const saveChain = (unit: FxUnitRef, name: string) =>
  updatePresets(commands.fxSaveChain(unit, name));

export const deleteChain = (name: string) => updatePresets(commands.fxDeleteChain(name));
