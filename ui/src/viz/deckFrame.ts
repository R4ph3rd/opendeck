import { telemetryStore } from "../state/telemetry";
import { nextAnchor, playheadAt, type PlayheadAnchor } from "./interp";

/** Everything the deck visuals read per animation frame. Seconds, not engine frames. */
export interface DeckFrame {
  loaded: boolean;
  playing: boolean;
  /** Interpolated playhead, seconds. */
  position: number;
  duration: number;
  /** Playback rate, 1 = original tempo. */
  rate: number;
  /** Loop region in seconds, `null` when the deck has none. */
  loop: { start: number; end: number } | null;
  /** The playhead is inside the loop right now. */
  loopActive: boolean;
  /** Changes with every new snapshot; lets draw code skip identical frames. */
  sequence: number;
}

/** Called once per animation frame with `performance.now()`. Must not allocate per call beyond the result. */
export type DeckFrameSource = (now: number) => DeckFrame;

export const EMPTY_DECK_FRAME: DeckFrame = {
  loaded: false,
  playing: false,
  position: 0,
  duration: 0,
  rate: 1,
  loop: null,
  loopActive: false,
  sequence: -1,
};

interface DeckClock {
  sequence: number;
  anchor: PlayheadAnchor | null;
  frame: DeckFrame;
}

/**
 * Frame source for one deck, reading the non-React telemetry store. It keeps
 * a playhead anchor per source and extrapolates between snapshots. Create it
 * once per component (not per frame).
 */
export function telemetryDeckFrameSource(deck: number): DeckFrameSource {
  const clock: DeckClock = { sequence: -2, anchor: null, frame: EMPTY_DECK_FRAME };
  return (now) => {
    const telemetry = telemetryStore.getState().telemetry;
    const d = telemetry?.decks[deck];
    if (!telemetry || !d) {
      clock.anchor = null;
      clock.sequence = -2;
      clock.frame = EMPTY_DECK_FRAME;
      return clock.frame;
    }
    const rateHz = telemetry.sample_rate > 0 ? telemetry.sample_rate : 1;
    if (telemetry.sequence !== clock.sequence) {
      clock.sequence = telemetry.sequence;
      const rate = d.rate ?? 1;
      clock.anchor = nextAnchor(clock.anchor, d.position / rateHz, rate, d.playing, now);
    }
    const anchor = clock.anchor;
    if (!anchor) {
      return EMPTY_DECK_FRAME;
    }
    const duration = d.duration / rateHz;
    const position = Math.min(playheadAt(anchor, now), duration);
    const region = d.loop_region;
    const f = clock.frame;
    // Reuse the object when the loop did not change to keep the frame loop allocation-light.
    const loop =
      region && region.end !== null
        ? f.loop && f.loop.start === region.start / rateHz && f.loop.end === region.end / rateHz
          ? f.loop
          : { start: region.start / rateHz, end: region.end / rateHz }
        : null;
    clock.frame = {
      loaded: d.loaded,
      playing: d.playing,
      position,
      duration,
      rate: anchor.rate,
      loop,
      loopActive: d.loop_active,
      sequence: telemetry.sequence,
    };
    return clock.frame;
  };
}
