import { create } from "zustand";
import { commands } from "../ipc/bindings";
import { errorMessage } from "./actions";
import { notifyError } from "./notify";
import { telemetryStore } from "./telemetry";

/**
 * The track preview (audition) from the collection. The engine has one
 * preview voice, so there is at most one previewed track. The backend answers
 * a start with a token; the engine telemetry carries the same token while that
 * track's buffer is the loaded one, which is how the UI tells its own track
 * from the previous one while a new buffer is still decoding.
 */
interface PreviewState {
  /** Track being auditioned or loading; `null` when idle. */
  trackId: number | null;
  /** Token the backend gave for it; `null` until the start call returned. */
  token: number | null;
  /** Telemetry has shown this track playing since the last start. */
  heard: boolean;
}

const idle: PreviewState = { trackId: null, token: null, heard: false };

export const usePreviewStore = create<PreviewState>()(() => idle);

/** Counts start calls so a late answer for an older click is ignored. */
let serial = 0;

/** Starts previewing `track` at `fraction` (`0..=1`) of its length, or jumps there if it already plays. */
export async function startPreview(track: number, fraction: number): Promise<void> {
  const mine = ++serial;
  usePreviewStore.setState({ trackId: track, token: null, heard: false });
  try {
    const result = await commands.previewTrack(track, Math.min(1, Math.max(0, fraction)));
    if (mine !== serial) {
      return;
    }
    if (result.status === "ok") {
      usePreviewStore.setState({ token: result.data });
    } else {
      usePreviewStore.setState(idle);
      notifyError(errorMessage(result.error));
    }
  } catch (error) {
    if (mine === serial) {
      usePreviewStore.setState(idle);
      notifyError(errorMessage(error));
    }
  }
}

/** Stops the preview. */
export function stopPreview(): void {
  serial += 1;
  usePreviewStore.setState(idle);
  void Promise.resolve(commands.previewStop()).catch(() => undefined);
}

/** Forgets the preview without telling the engine (it ended or failed there). */
export function clearPreview(track?: number): void {
  if (track === undefined || usePreviewStore.getState().trackId === track) {
    serial += 1;
    usePreviewStore.setState(idle);
  }
}

/** A click on the mini waveform of `track`: stops its preview, or starts one at `fraction`. */
export function togglePreview(track: number, fraction: number): void {
  if (usePreviewStore.getState().trackId === track) {
    stopPreview();
  } else {
    void startPreview(track, fraction);
  }
}

/**
 * Follows the engine: once the track has been heard playing, a stop in
 * telemetry means it reached its end. Runs on every snapshot but does nothing
 * while idle.
 */
telemetryStore.subscribe((state) => {
  const { trackId, token, heard } = usePreviewStore.getState();
  const preview = state.telemetry?.preview;
  if (trackId === null || token === null || !preview || preview.token !== token) {
    return;
  }
  if (preview.playing) {
    if (!heard) {
      usePreviewStore.setState({ heard: true });
    }
  } else if (heard) {
    clearPreview(trackId);
  }
});
