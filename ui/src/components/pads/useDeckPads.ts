import { useStore } from "zustand";
import type { DeckPadStateDto } from "../../ipc/bindings";
import { usePadStore } from "../../state/pads";
import { telemetryStore } from "../../state/telemetry";
import { selectDeckUi, useAppStore } from "../../state/app";
import { loopBeats } from "../../lib/pads";

/** Pad state of one deck from the backend, `undefined` until it arrives. */
export function useDeckPads(deck: number): DeckPadStateDto | undefined {
  return usePadStore((s) => s.pad?.decks.find((d) => d.deck === deck));
}

/** Whether a track is loaded and playable in `deck`. */
export function useDeckLoaded(deck: number): boolean {
  return useStore(telemetryStore, (s) => s.telemetry?.decks[deck]?.loaded ?? false);
}

/** Length in beats of the loop playing on `deck`, `null` when not looping. */
export function useActiveLoopBeats(deck: number): number | null {
  const track = useAppStore(selectDeckUi(deck)).track;
  const bpm = track?.bpm ?? null;
  return useStore(telemetryStore, (s) => {
    const d = s.telemetry?.decks[deck];
    if (!d?.loop_active) {
      return null;
    }
    return loopBeats(
      d.loop_region ? { start: d.loop_region.start, end: d.loop_region.end } : null,
      s.telemetry?.sample_rate ?? 0,
      bpm,
    );
  });
}
