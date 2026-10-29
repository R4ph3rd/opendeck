import { create } from "zustand";
import {
  commands,
  type CrossfaderAssign,
  type CrossfaderCurve,
  type EqBand,
  type MixerState,
} from "../ipc/bindings";
import {
  CROSSFADER_DEFAULT,
  EQ_DEFAULT,
  FADER_DEFAULT,
  FILTER_DEFAULT,
  HEADPHONE_MIX_DEFAULT,
  LEVEL_DEFAULT,
  TRIM_DEFAULT,
  eqGain,
  eqPosition,
  levelGain,
  levelPosition,
  trimGain,
  trimPosition,
} from "../lib/mixer";
import { report } from "./actions";

/** EQ bands in the order the engine indexes them (`EqBand::index`). */
export const EQ_BANDS: readonly EqBand[] = ["low", "mid", "high"];

export const CROSSFADER_CURVES: readonly CrossfaderCurve[] = ["constant_power", "linear", "sharp"];

export const CROSSFADER_ASSIGNS: readonly CrossfaderAssign[] = ["a", "thru", "b"];

/** What one channel strip shows. Knobs hold normalised positions (`lib/mixer.ts`). */
export interface ChannelUi {
  trim: number;
  /** Indexed like `EQ_BANDS`: low, mid, high. */
  eq: [number, number, number];
  /** `-1..1`, negative low-pass, positive high-pass. */
  filter: number;
  /** `0..1`. */
  fader: number;
  pfl: boolean;
  assign: CrossfaderAssign;
}

export interface MixerUi {
  channels: ChannelUi[];
  /** `-1..1`, `-1` = full A. */
  crossfader: number;
  curve: CrossfaderCurve;
  master: number;
  headphone: number;
  headphoneMix: number;
  splitCue: boolean;
}

const neutralChannel = (): ChannelUi => ({
  trim: TRIM_DEFAULT,
  eq: [EQ_DEFAULT, EQ_DEFAULT, EQ_DEFAULT],
  filter: FILTER_DEFAULT,
  fader: FADER_DEFAULT,
  pfl: false,
  assign: "thru",
});

const initial = (): MixerUi => ({
  channels: [neutralChannel(), neutralChannel()],
  crossfader: CROSSFADER_DEFAULT,
  curve: "constant_power",
  master: LEVEL_DEFAULT,
  headphone: LEVEL_DEFAULT,
  headphoneMix: HEADPHONE_MIX_DEFAULT,
  splitCue: false,
});

interface MixerStore extends MixerUi {
  patchChannel: (deck: number, patch: Partial<ChannelUi>) => void;
  patch: (patch: Partial<MixerUi>) => void;
  /** Applies the backend's mixer snapshot (startup). */
  apply: (state: MixerState) => void;
  reset: () => void;
}

export const useMixerStore = create<MixerStore>()((set) => ({
  ...initial(),
  patchChannel: (deck, patch) =>
    set((state) => ({
      channels: state.channels.map((c, i) => (i === deck ? { ...c, ...patch } : c)),
    })),
  patch: (patch) => set(patch),
  apply: (mixer) =>
    set({
      channels: [0, 1].map((i) => {
        const c = mixer.channels[i];
        if (!c) {
          return neutralChannel();
        }
        return {
          trim: trimPosition(c.trim ?? 1),
          eq: [eqPosition(c.eq[0] ?? 1), eqPosition(c.eq[1] ?? 1), eqPosition(c.eq[2] ?? 1)],
          filter: c.filter ?? 0,
          fader: c.fader ?? 1,
          pfl: c.pfl,
          assign: c.crossfader_assign,
        };
      }),
      crossfader: mixer.crossfader ?? 0,
      curve: mixer.crossfader_curve,
      master: levelPosition(mixer.master_level ?? 1),
      headphone: levelPosition(mixer.headphone_level ?? 1),
      headphoneMix: mixer.headphone_mix ?? 0,
      splitCue: mixer.split_cue,
    }),
  reset: () => set(initial()),
}));

const store = () => useMixerStore.getState();

export function setTrim(deck: number, position: number): void {
  store().patchChannel(deck, { trim: position });
  void report(commands.setTrim(deck, trimGain(position)));
}

export function setEq(deck: number, band: EqBand, position: number): void {
  const index = EQ_BANDS.indexOf(band);
  const channel = store().channels[deck];
  if (!channel || index < 0) {
    return;
  }
  const eq: [number, number, number] = [...channel.eq];
  eq[index] = position;
  store().patchChannel(deck, { eq });
  void report(commands.setEq(deck, band, eqGain(position)));
}

export function setFilter(deck: number, amount: number): void {
  store().patchChannel(deck, { filter: amount });
  void report(commands.setFilter(deck, amount));
}

export function setChannelFader(deck: number, position: number): void {
  store().patchChannel(deck, { fader: position });
  void report(commands.setChannelFader(deck, position));
}

export function setPfl(deck: number, on: boolean): void {
  store().patchChannel(deck, { pfl: on });
  void report(commands.setPfl(deck, on));
}

export function setCrossfaderAssign(deck: number, assign: CrossfaderAssign): void {
  store().patchChannel(deck, { assign });
  void report(commands.setCrossfaderAssign(deck, assign));
}

export function setCrossfader(position: number): void {
  store().patch({ crossfader: position });
  void report(commands.setCrossfader(position));
}

export function setCrossfaderCurve(curve: CrossfaderCurve): void {
  store().patch({ curve });
  void report(commands.setCrossfaderCurve(curve));
}

export function setMasterLevel(position: number): void {
  store().patch({ master: position });
  void report(commands.setMasterLevel(levelGain(position)));
}

export function setHeadphoneLevel(position: number): void {
  store().patch({ headphone: position });
  void report(commands.setHeadphoneLevel(levelGain(position)));
}

export function setHeadphoneMix(mix: number): void {
  store().patch({ headphoneMix: mix });
  void report(commands.setHeadphoneMix(mix));
}

export function setSplitCue(on: boolean): void {
  store().patch({ splitCue: on });
  void report(commands.setSplitCue(on));
}

/** Reads the backend's mixer into the store (startup). */
export async function hydrateMixer(): Promise<void> {
  const mixer = await report(commands.getMixerState());
  if (mixer) {
    store().apply(mixer);
  }
}
