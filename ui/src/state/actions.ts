import { open } from "@tauri-apps/plugin-dialog";
import { commands, type CommandError, type SyncMode, type TempoRange } from "../ipc/bindings";
import { framesToSeconds, telemetryStore } from "./telemetry";
import { DEFAULT_JUMP_BEATS, selectDeckUi, useAppStore, type DeckUi } from "./app";
import { notifyError } from "./notify";

type Outcome<T> = { status: "ok"; data: T } | { status: "error"; error: CommandError | string };

/** Human-readable text for whatever a rejected command carries. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return "Something went wrong.";
}

/** Unwraps a command result; shows an error toast and returns `null` on failure. */
export async function report<T>(pending: Promise<Outcome<T>>): Promise<T | null> {
  try {
    const result = await pending;
    if (result.status === "ok") {
      return result.data;
    }
    notifyError(errorMessage(result.error));
  } catch (error) {
    notifyError(errorMessage(error));
  }
  return null;
}

/** Loads `path` into `deck`. The deck shows the tags at once; audio follows on `track-loaded`. */
export async function loadFile(deck: number, path: string): Promise<void> {
  const { patchDeck } = useAppStore.getState();
  patchDeck(deck, { loading: true });
  const info = await report(commands.loadTrack(deck, path));
  patchDeck(deck, info ? { incoming: info } : { loading: false });
}

/** Opens the native file picker and loads the choice into `deck`. */
export async function pickAndLoad(deck: number): Promise<void> {
  let extensions: string[] = [];
  try {
    extensions = await commands.supportedAudioExtensions();
  } catch {
    // Without the list the dialog shows every file; the decoder still validates.
  }
  const filters = extensions.length > 0 ? [{ name: "Audio", extensions }] : [];
  let picked: string | string[] | null;
  try {
    picked = await open({ multiple: false, directory: false, filters });
  } catch (error) {
    notifyError(errorMessage(error));
    return;
  }
  if (typeof picked === "string") {
    await loadFile(deck, picked);
  }
}

export async function ejectDeck(deck: number): Promise<void> {
  const done = await report(commands.ejectTrack(deck));
  if (done !== null) {
    useAppStore.getState().patchDeck(deck, { track: null, incoming: null, loading: false });
  }
}

export function togglePlay(deck: number): void {
  void report(commands.togglePlaying(deck));
}

const cueHeld = new Set<number>();

/** CUE pressed. Repeats while held (key repeat, double events) are ignored. */
export function cuePress(deck: number): void {
  if (cueHeld.has(deck)) {
    return;
  }
  cueHeld.add(deck);
  void report(commands.cue(deck, true));
}

/** CUE released; a release without a press is ignored. */
export function cueRelease(deck: number): void {
  if (!cueHeld.delete(deck)) {
    return;
  }
  void report(commands.cue(deck, false));
}

/** Releases every held CUE (window blur, unmount) so a deck never stays in preview. */
export function releaseAllCues(): void {
  for (const deck of [...cueHeld]) {
    cueRelease(deck);
  }
}

/** Seeks to `fraction` (0..1) of the loaded track using the newest telemetry. */
export function seekFraction(deck: number, fraction: number): void {
  const t = telemetryStore.getState().telemetry;
  const d = t?.decks[deck];
  if (!t || !d?.loaded) {
    return;
  }
  const duration = framesToSeconds(d.duration, t.sample_rate);
  const clamped = Math.min(1, Math.max(0, fraction));
  void report(commands.seek(deck, clamped * duration));
}

/** Seeks `delta` seconds from the current position. */
export function seekBy(deck: number, delta: number): void {
  const t = telemetryStore.getState().telemetry;
  const d = t?.decks[deck];
  if (!t || !d?.loaded) {
    return;
  }
  const duration = framesToSeconds(d.duration, t.sample_rate);
  const position = framesToSeconds(d.position, t.sample_rate) + delta;
  void report(commands.seek(deck, Math.min(duration, Math.max(0, position))));
}

export function setTempoFader(deck: number, position: number): void {
  useAppStore.getState().patchDeck(deck, { tempoFader: position });
  void report(commands.setTempoFader(deck, position));
}

export function setTempoRange(deck: number, range: TempoRange): void {
  useAppStore.getState().patchDeck(deck, { tempoRange: range });
  void report(commands.setTempoRange(deck, range));
}

export function setKeyLock(deck: number, on: boolean): void {
  useAppStore.getState().patchDeck(deck, { keyLock: on });
  void report(commands.setKeyLock(deck, on));
}

/** Pitch bend while a button is held: `amount` is -1 or 1, 0 releases. */
export function pitchBend(deck: number, amount: number): void {
  void report(commands.pitchBend(deck, amount));
}

// ----- sync, quantize, slip (milestone 2) -----

export function setSync(deck: number, mode: SyncMode): void {
  const patch: Partial<DeckUi> = mode === "off" ? { sync: mode } : { sync: mode, syncPref: mode };
  useAppStore.getState().patchDeck(deck, patch);
  void report(commands.setSync(deck, mode));
}

/** SYNC button: off turns sync on in the last chosen mode, on turns it off. */
export function toggleSync(deck: number): void {
  const ui = selectDeckUi(deck)(useAppStore.getState());
  setSync(deck, ui.sync === "off" ? ui.syncPref : "off");
}

/** Makes `deck` the sync leader, or hands the choice back to the engine with `null`. */
export function setLeader(deck: number | null): void {
  useAppStore.getState().setLeader(deck);
  void report(commands.setLeader(deck));
}

/** MASTER button: `isLeader` is what the button currently shows. */
export function toggleLeader(deck: number, isLeader: boolean): void {
  setLeader(isLeader ? null : deck);
}

export function setQuantize(deck: number, on: boolean): void {
  useAppStore.getState().patchDeck(deck, { quantize: on });
  void report(commands.setQuantize(deck, on));
}

export function toggleQuantize(deck: number): void {
  setQuantize(deck, !selectDeckUi(deck)(useAppStore.getState()).quantize);
}

export function setSlip(deck: number, on: boolean): void {
  useAppStore.getState().patchDeck(deck, { slip: on });
  void report(commands.setSlip(deck, on));
}

export function toggleSlip(deck: number): void {
  setSlip(deck, !selectDeckUi(deck)(useAppStore.getState()).slip);
}

// ----- loops and beat jump -----

export function loopIn(deck: number): void {
  void report(commands.loopIn(deck));
}

export function loopOut(deck: number): void {
  void report(commands.loopOut(deck));
}

/** RELOOP / EXIT: leaves the active loop or re-enters the last one. */
export function loopToggle(deck: number): void {
  void report(commands.loopToggle(deck));
}

export function autoLoop(deck: number, beats: number): void {
  void report(commands.autoLoop(deck, beats));
}

export function resizeLoop(deck: number, factor: number): void {
  void report(commands.resizeLoop(deck, factor));
}

const rollHeld = new Map<number, number>();

/** Loop roll pressed. A second press on the same deck while one is held is ignored. */
export function loopRollPress(deck: number, beats: number): void {
  if (rollHeld.has(deck)) {
    return;
  }
  rollHeld.set(deck, beats);
  void report(commands.loopRoll(deck, beats, true));
}

/** Loop roll released; a release without a press is ignored. */
export function loopRollRelease(deck: number): void {
  const beats = rollHeld.get(deck);
  if (beats === undefined) {
    return;
  }
  rollHeld.delete(deck);
  void report(commands.loopRoll(deck, beats, false));
}

export function setRollMode(deck: number, on: boolean): void {
  useAppStore.getState().patchDeck(deck, { rollMode: on });
}

export function setJumpBeats(deck: number, beats: number): void {
  useAppStore.getState().patchDeck(deck, { jumpBeats: beats > 0 ? beats : DEFAULT_JUMP_BEATS });
}

/** Jumps the selected beat count forward (`1`) or backward (`-1`). */
export function beatJump(deck: number, direction: 1 | -1): void {
  const { jumpBeats } = selectDeckUi(deck)(useAppStore.getState());
  void report(commands.beatJump(deck, direction * jumpBeats));
}

// ----- hot cues -----

const hotCueHeld = new Set<string>();
const hotCueKey = (deck: number, index: number): string => `${deck}:${index}`;

/**
 * Hot cue pad pressed. Empty slot: stores the playhead. Set slot: jumps (and
 * previews while held when paused). With `shift`, a set slot is deleted.
 */
export function hotCuePress(deck: number, index: number, shift = false): void {
  const slot = selectDeckUi(deck)(useAppStore.getState()).hotCues[index] ?? null;
  if (slot === null) {
    void report(commands.setHotCue(deck, index));
    return;
  }
  if (shift) {
    void report(commands.deleteHotCue(deck, index));
    return;
  }
  const key = hotCueKey(deck, index);
  if (hotCueHeld.has(key)) {
    return;
  }
  hotCueHeld.add(key);
  void report(commands.triggerHotCue(deck, index, true));
}

/** Hot cue pad released; a release without a trigger press is ignored. */
export function hotCueRelease(deck: number, index: number): void {
  if (hotCueHeld.delete(hotCueKey(deck, index))) {
    void report(commands.triggerHotCue(deck, index, false));
  }
}

/** Releases every held hot cue and loop roll (window blur, unmount). */
export function releaseAllHeld(): void {
  releaseAllCues();
  for (const deck of [...rollHeld.keys()]) {
    loopRollRelease(deck);
  }
  for (const key of [...hotCueHeld]) {
    const [deck, index] = key.split(":").map(Number);
    if (deck !== undefined && index !== undefined) {
      hotCueRelease(deck, index);
    }
  }
}

/** Reads the backend's deck state into the store (startup). */
export async function hydrateDeck(deck: number): Promise<void> {
  const state = await report(commands.getDeckState(deck));
  if (state) {
    useAppStore.getState().applyDeckState(state);
  }
}
