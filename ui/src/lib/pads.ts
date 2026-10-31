import type { PadMode, SamplerPlayMode } from "../ipc/bindings";

/** Mode names as the controller labels them. */
export const MODE_LABEL: Record<PadMode, string> = {
  hot_cue: "Hot cue",
  pad_fx1: "Pad FX 1",
  beat_jump: "Beat jump",
  sampler: "Sampler",
  keyboard: "Keyboard",
  pad_fx2: "Pad FX 2",
  beat_loop: "Beat loop",
  key_shift: "Key shift",
};

/**
 * The four mode buttons of a deck, each with its SHIFT partner (manual p.17-18,
 * `PadMode::shift_partner`): HOT CUE / Keyboard, PAD FX 1 / PAD FX 2,
 * BEAT JUMP / Beat loop, SAMPLER / Key shift.
 */
export const MODE_BUTTONS: readonly (readonly [PadMode, PadMode])[] = [
  ["hot_cue", "keyboard"],
  ["pad_fx1", "pad_fx2"],
  ["beat_jump", "beat_loop"],
  ["sampler", "key_shift"],
];

/** The unshifted mode of the button that selects `mode`. */
export function baseMode(mode: PadMode): PadMode {
  return MODE_BUTTONS.find(([a, b]) => a === mode || b === mode)?.[0] ?? "hot_cue";
}

/** The mode on the same button with the other SHIFT state. */
export function partnerMode(mode: PadMode): PadMode {
  const pair = MODE_BUTTONS.find(([a, b]) => a === mode || b === mode);
  return pair ? (pair[0] === mode ? pair[1] : pair[0]) : mode;
}

/** `+3`, `±0`, `-2` (semitones). */
export function semitoneLabel(semitones: number): string {
  if (semitones === 0) {
    return "±0";
  }
  return semitones > 0 ? `+${semitones}` : `${semitones}`;
}

/** `1/4`, `1/2`, `1`, `32` (beats). */
export function beatsLabel(beats: number): string {
  const abs = Math.abs(beats);
  if (abs === 0.25) return "1/4";
  if (abs === 0.5) return "1/2";
  return String(abs);
}

/** `← 2` or `2 →` for a signed beat jump. */
export function jumpLabel(beats: number): string {
  return beats < 0 ? `← ${beatsLabel(beats)}` : `${beatsLabel(beats)} →`;
}

export const PLAY_MODES: readonly { value: SamplerPlayMode; label: string }[] = [
  { value: "one_shot", label: "One-shot" },
  { value: "gate", label: "Gate" },
  { value: "loop", label: "Loop" },
];

/** Slots a deck's pads reach: deck 1 slots 1-8 and deck 2 slots 9-16 in split layout. */
export function deckSlots(layout: "split" | "shared", deck: number): number[] {
  const base = layout === "split" ? deck * 8 : 0;
  return Array.from({ length: 8 }, (_, i) => base + i);
}

/** `1:30.5` style length of a sample. */
export function sampleLength(seconds: number): string {
  if (seconds < 60) {
    return `${seconds.toFixed(1)} s`;
  }
  const m = Math.floor(seconds / 60);
  return `${m}:${String(Math.floor(seconds % 60)).padStart(2, "0")}`;
}

/** Beats of a loop from its frame region and the track tempo; `null` when unknown. */
export function loopBeats(
  region: { start: number; end: number | null } | null,
  sampleRate: number,
  bpm: number | null | undefined,
): number | null {
  if (!region || region.end === null || !bpm || bpm <= 0 || sampleRate <= 0) {
    return null;
  }
  return ((region.end - region.start) / sampleRate) * (bpm / 60);
}

/** Duration of the view cross-fade when the pad mode changes, seconds. */
export const MODE_FADE_SECONDS = 0.12;

/** Layouts before the backend reports them (`app-core` `pads.rs`, manual p.60, p.63-65). */
export const DEFAULT_JUMP_BEATS: readonly number[] = [-1, 1, -2, 2, -4, 4, -8, 8];
export const DEFAULT_LOOP_BEATS: readonly number[] = [0.25, 0.5, 1, 2, 4, 8, 16, 32];
export const DEFAULT_SEMITONES: readonly number[] = [4, 5, 6, 7, 0, 1, 2, 3];
