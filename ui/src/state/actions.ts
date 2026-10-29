import { open } from "@tauri-apps/plugin-dialog";
import { commands, type CommandError, type TempoRange } from "../ipc/bindings";
import { framesToSeconds, telemetryStore } from "./telemetry";
import { useAppStore } from "./app";

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
    useAppStore.getState().pushToast("error", errorMessage(result.error));
  } catch (error) {
    useAppStore.getState().pushToast("error", errorMessage(error));
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
    useAppStore.getState().pushToast("error", errorMessage(error));
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
