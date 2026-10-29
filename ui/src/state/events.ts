import type { UnlistenFn } from "@tauri-apps/api/event";
import { commands, events, type EngineEvent } from "../ipc/bindings";
import { errorMessage } from "./actions";
import { useAppStore } from "./app";
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
  const app = useAppStore.getState();
  const unlisten: UnlistenFn[] = [];
  try {
    unlisten.push(
      await events.engineTelemetry.listen((e) => telemetryStore.getState().push(e.payload)),
      await events.engineEvent.listen((e) => {
        const message = engineEventMessage(e.payload);
        if (message) {
          useAppStore.getState().pushToast("error", message);
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
        const state = useAppStore.getState();
        state.patchDeck(e.payload.deck, { incoming: null, loading: false });
        state.pushToast("error", `Could not load ${fileName(e.payload.path)}: ${e.payload.message}`);
      }),
      await events.audioDeviceChanged.listen((e) => useAppStore.getState().setAudio(e.payload)),
    );
    // Subscribe first, then read: a change between the two is not lost.
    const status = await commands.getAudioStatus();
    if (status.status === "ok") {
      useAppStore.getState().setAudio(status.data);
    }
  } catch (error) {
    app.pushToast("error", errorMessage(error));
  }
  return () => {
    for (const stop of unlisten) {
      stop();
    }
  };
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}
