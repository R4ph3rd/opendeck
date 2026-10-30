import type { UnlistenFn } from "@tauri-apps/api/event";
import { commands, events, type EngineEvent } from "../ipc/bindings";
import { errorMessage, hydrateDeck, report } from "./actions";
import { VISIBLE_DECKS, useAppStore } from "./app";
import { useLibraryStore } from "./library";
import { hydrateJobs, onImportProgress, refreshPlaylists } from "./libraryActions";
import { hydrateMixer } from "./mixer";
import { notifyError } from "./notify";
import { telemetryStore } from "./telemetry";

/** Message for engine events the user should see; `null` for the quiet ones. */
export function engineEventMessage(event: EngineEvent): string | null {
  switch (event.type) {
    case "device_lost":
      return "Audio device lost. Choose an output in Settings to restart audio.";
    case "stream_error":
      return "The audio stream reported an error.";
    case "device_changed":
      return "The audio device changed.";
    default:
      return null;
  }
}

/**
 * Subscribes to every backend event and routes it to the stores. Returns a
 * function that removes all listeners. Safe to call before the backend is up:
 * a failure to listen shows a toast instead of throwing.
 */
export async function startEventBridge(): Promise<() => void> {
  const unlisten: UnlistenFn[] = [];
  try {
    unlisten.push(
      await events.engineTelemetry.listen((e) => telemetryStore.getState().push(e.payload)),
      await events.engineEvent.listen((e) => {
        const message = engineEventMessage(e.payload);
        if (message) {
          notifyError(message);
        }
      }),
      await events.trackLoaded.listen((e) => {
        useAppStore.getState().patchDeck(e.payload.deck, {
          track: e.payload.track,
          incoming: null,
          loading: false,
        });
      }),
      await events.trackLoadFailed.listen((e) => {
        useAppStore.getState().patchDeck(e.payload.deck, { incoming: null, loading: false });
        notifyError(`Could not load ${fileName(e.payload.path)}: ${e.payload.message}`);
      }),
      await events.hotCuesChanged.listen((e) => {
        useAppStore.getState().patchDeck(e.payload.deck, { hotCues: e.payload.slots });
      }),
      await events.audioDeviceChanged.listen((e) => useAppStore.getState().setAudio(e.payload)),
      await events.importProgress.listen((e) => onImportProgress(e.payload)),
      await events.libraryChanged.listen(() => {
        useLibraryStore.getState().bumpRevision();
        // Imports can create playlists; keep the tree and its counts current.
        void refreshPlaylists();
      }),
      await events.analysisReady.listen((e) => void refreshDeckAnalysis(e.payload)),
    );
    // Subscribe first, then read: a change between the two is not lost.
    const status = await commands.getAudioStatus();
    if (status.status === "ok") {
      useAppStore.getState().setAudio(status.data);
    }
    await Promise.all([
      hydrateMixer(),
      hydrateJobs(),
      ...VISIBLE_DECKS.map((deck) => hydrateDeck(deck)),
    ]);
  } catch (error) {
    notifyError(errorMessage(error));
  }
  return () => {
    for (const stop of unlisten) {
      stop();
    }
  };
}

/** Refreshes the BPM and key a deck shows when analysis of its track finishes. */
export async function refreshDeckAnalysis(track: number): Promise<void> {
  const holders = VISIBLE_DECKS.filter((deck) => {
    const ui = useAppStore.getState().decks[deck];
    return (ui?.track ?? ui?.incoming)?.id === track;
  });
  if (holders.length === 0) {
    return;
  }
  const analysis = await report(commands.getAnalysis(track));
  if (!analysis) {
    return;
  }
  for (const deck of holders) {
    const ui = useAppStore.getState().decks[deck];
    const info = ui?.track ?? ui?.incoming;
    if (info?.id !== track) {
      continue;
    }
    const patch = { ...info, bpm: analysis.bpm, key: analysis.key, key_musical: analysis.key_musical };
    useAppStore.getState().patchDeck(deck, ui?.track ? { track: patch } : { incoming: patch });
  }
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
