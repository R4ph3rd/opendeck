import { useEffect, useMemo, useState } from "react";
import { create } from "zustand";
import { commands, events, type BeatGrid, type Cue } from "../ipc/bindings";
import { selectDeckUi, useAppStore } from "../state/app";
import { EMPTY_PEAKS, peakCache, type TrackPeaks } from "./peakCache";
import { NO_MARKERS, type DeckMarkers } from "./markers";
import { DEFAULT_ZOOM_INDEX, clampZoom } from "./waveformMath";

/** Retry delay while a track has no peaks yet (analysis running), ms. */
const PEAK_RETRY_MS = 2000;
const PEAK_RETRIES = 30;

/** Peaks of a library track, `EMPTY_PEAKS` until analysed. Reloads when analysis finishes. */
export function useTrackPeaks(trackId: number | null): TrackPeaks {
  const [state, setState] = useState<{ id: number | null; peaks: TrackPeaks }>({
    id: null,
    peaks: EMPTY_PEAKS,
  });
  useEffect(() => {
    if (trackId === null) {
      return undefined;
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let attempts = 0;
    const load = async () => {
      const peaks = await peakCache.track(trackId, (partial) => {
        if (!cancelled) setState({ id: trackId, peaks: partial });
      });
      if (cancelled) return;
      setState({ id: trackId, peaks });
      const complete = peaks.overview !== null && peaks.detail.every((l) => l !== null);
      if (!complete && attempts < PEAK_RETRIES) {
        attempts += 1;
        timer = setTimeout(() => void load(), PEAK_RETRY_MS);
      }
    };
    void load();
    let unlisten: (() => void) | undefined;
    void events.analysisReady
      .listen((e) => {
        if (e.payload === trackId) {
          attempts = 0;
          void load();
        }
      })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      unlisten?.();
    };
  }, [trackId]);
  return state.id === trackId ? state.peaks : EMPTY_PEAKS;
}

interface TrackExtras {
  id: number | null;
  grid: BeatGrid | null;
  cues: Cue[];
}

/**
 * Beat grid, hot cues and memory cues of the track in `deck`. Hot cues come
 * from the app store (the engine's slots); grid and memory cues are fetched
 * per track and refetched on `analysis-ready` and `cues-changed`.
 */
export function useDeckMarkers(deck: number): DeckMarkers {
  const ui = useAppStore(selectDeckUi(deck));
  const trackId = ui.track?.id ?? null;
  const [extras, setExtras] = useState<TrackExtras>({ id: null, grid: null, cues: [] });

  useEffect(() => {
    if (trackId === null) {
      return undefined;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const [analysis, cues] = await Promise.all([
          commands.getAnalysis(trackId),
          commands.listCues(trackId),
        ]);
        if (cancelled) return;
        setExtras({
          id: trackId,
          grid: analysis.status === "ok" ? (analysis.data?.beat_grid ?? null) : null,
          cues: cues.status === "ok" ? cues.data : [],
        });
      } catch {
        // Leave the previous markers; the next event retries.
      }
    };
    void load();
    const stops: (() => void)[] = [];
    const watch = (listen: Promise<() => void>) =>
      listen
        .then((stop) => {
          if (cancelled) stop();
          else stops.push(stop);
        })
        .catch(() => undefined);
    void watch(
      events.analysisReady.listen((e) => {
        if (e.payload === trackId) void load();
      }),
    );
    void watch(
      events.cuesChanged.listen((e) => {
        if (e.payload === trackId) void load();
      }),
    );
    return () => {
      cancelled = true;
      for (const stop of stops) stop();
    };
  }, [trackId]);

  const current = extras.id === trackId ? extras : null;
  return useMemo(() => {
    if (trackId === null) {
      return NO_MARKERS;
    }
    return {
      grid: current?.grid ?? null,
      hotCues: ui.hotCues,
      memoryCues: (current?.cues ?? []).filter((c) => c.kind.kind !== "hot"),
    };
  }, [trackId, current, ui.hotCues]);
}

/** Per-deck viz settings that are UI-only. */
interface VizState {
  zoom: number[];
  setZoom: (deck: number, index: number) => void;
}

export const useVizStore = create<VizState>()((set) => ({
  zoom: [DEFAULT_ZOOM_INDEX, DEFAULT_ZOOM_INDEX, DEFAULT_ZOOM_INDEX, DEFAULT_ZOOM_INDEX],
  setZoom: (deck, index) =>
    set((s) => ({ zoom: s.zoom.map((z, i) => (i === deck ? clampZoom(index) : z)) })),
}));
