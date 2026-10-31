import { create } from "zustand";
import {
  commands,
  type PadMode,
  type PadStateDto,
  type SamplerLayout,
  type SamplerSlotPatch,
  type SamplerStateDto,
} from "../ipc/bindings";
import { hydrateDeck, report } from "./actions";
import { useAppStore } from "./app";

interface PadStore {
  /** Beat jump sizes, pitch layouts and the keyboard cue of every deck. */
  pad: PadStateDto | null;
  sampler: SamplerStateDto | null;
  setPad: (pad: PadStateDto) => void;
  setSampler: (sampler: SamplerStateDto) => void;
  reset: () => void;
}

export const usePadStore = create<PadStore>()((set) => ({
  pad: null,
  sampler: null,
  setPad: (pad) => set({ pad }),
  setSampler: (sampler) => set({ sampler }),
  reset: () => set({ pad: null, sampler: null }),
}));

/** Reads the pad and sampler state once at startup. */
export async function hydratePads(): Promise<void> {
  const [pad, sampler] = await Promise.all([
    report(commands.getPadState()),
    report(commands.getSamplerState()),
  ]);
  if (pad) usePadStore.getState().setPad(pad);
  if (sampler) usePadStore.getState().setSampler(sampler);
}

/**
 * Switches the pad mode of a deck. The store changes at once; the backend
 * owns the state and the controller's LEDs follow it.
 */
export function selectPadMode(deck: number, mode: PadMode): void {
  useAppStore.getState().setPadMode(deck, mode);
  void report(commands.setPadMode(deck, mode));
}

const held = new Map<string, PadMode>();
const heldKey = (deck: number, pad: number): string => `${deck}:${pad}`;

/** Pad pressed on screen. `shift` is the SHIFT layer of the pad. */
export function padDown(deck: number, mode: PadMode, pad: number, shift: boolean): void {
  held.set(heldKey(deck, pad), mode);
  void report(commands.padPress(deck, mode, pad, shift, true)).then(() => {
    if (mode === "key_shift" || mode === "keyboard") {
      void hydrateDeck(deck);
    }
  });
}

/** Pad released. A release without a press is ignored. */
export function padUp(deck: number, pad: number, shift: boolean): void {
  const mode = held.get(heldKey(deck, pad));
  if (mode === undefined) {
    return;
  }
  held.delete(heldKey(deck, pad));
  void report(commands.padPress(deck, mode, pad, shift, false));
}

export function renameHotCue(deck: number, index: number, name: string): void {
  void report(commands.renameHotCue(deck, index, name));
}

/** Changes the named fields of a slot; the others keep their value. */
export function samplerPatch(slot: number, patch: Partial<SamplerSlotPatch>): void {
  const keep: SamplerSlotPatch = { name: null, color: null, gain: null, play_mode: null, sync: null };
  void report(commands.samplerSetSlot(slot, { ...keep, ...patch }));
}

export function samplerLoadTrack(slot: number, track: number): void {
  void report(commands.samplerLoadTrack(slot, track));
}

export function samplerLoadPath(slot: number, path: string): void {
  void report(commands.samplerLoadPath(slot, path));
}

export function samplerClear(slot: number): void {
  void report(commands.samplerClear(slot));
}

export function samplerLayout(layout: SamplerLayout): void {
  void report(commands.samplerSetLayout(layout));
}

export function samplerLevel(level: number): void {
  const sampler = usePadStore.getState().sampler;
  if (sampler) usePadStore.getState().setSampler({ ...sampler, level });
  void report(commands.samplerSetLevel(level));
}

export function samplerPfl(on: boolean): void {
  void report(commands.samplerSetPfl(on));
}

export function samplerTrigger(slot: number, pressed: boolean): void {
  void report(commands.samplerTrigger(slot, pressed));
}

export function samplerStop(slot: number): void {
  void report(commands.samplerStop(slot));
}
