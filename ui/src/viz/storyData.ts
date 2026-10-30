import { MockDeck, mockMarkers, synthPeaks } from "./mock";
import type { DeckMarkers } from "./markers";
import type { TrackPeaks } from "./peakCache";

const cache = new Map<number, { peaks: TrackPeaks; markers: DeckMarkers }>();

/** Synthetic track of `seconds`, built once per length. Same data on every run. */
export function storyTrack(seconds: number): { peaks: TrackPeaks; markers: DeckMarkers } {
  let hit = cache.get(seconds);
  if (!hit) {
    hit = { peaks: synthPeaks(seconds), markers: mockMarkers(seconds) };
    cache.set(seconds, hit);
  }
  return hit;
}

/** A fresh mock deck that starts a little into the track so the grid and cues are in view. */
export function storyDeck(seconds: number, over: ConstructorParameters<typeof MockDeck>[1] = {}): MockDeck {
  return new MockDeck(seconds, { position: 6, ...over });
}
