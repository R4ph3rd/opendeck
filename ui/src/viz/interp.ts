/**
 * Playhead interpolation. Telemetry arrives at about 60 Hz with arrival
 * jitter; the waveform runs on its own animation frames. Between snapshots the
 * playhead is extrapolated from the last position and the playback rate. When
 * a snapshot disagrees slightly with the extrapolation, the error is bled off
 * exponentially instead of jumping, so scrolling stays smooth. Large errors
 * (seeks, loops, cue jumps) snap.
 */

export interface PlayheadAnchor {
  /** Position at `at`, seconds of track time. */
  position: number;
  /** Track seconds per wall second. */
  rate: number;
  playing: boolean;
  /** `performance.now()` when the snapshot was seen, ms. */
  at: number;
  /** Extrapolation error to bleed off, seconds. */
  error: number;
}

/** Longest the playhead runs ahead without a new snapshot, ms. */
export const MAX_EXTRAPOLATION_MS = 250;
/** Time constant of the error bleed-off, ms. */
export const CORRECTION_TAU_MS = 80;
/** Errors above this snap instead of blending, seconds. */
export const SNAP_THRESHOLD_S = 0.2;

/** Position `anchor` predicts at `now`, without the correction term. */
export function predicted(anchor: PlayheadAnchor, now: number): number {
  const dt = Math.min(Math.max(now - anchor.at, 0), MAX_EXTRAPOLATION_MS) / 1000;
  return anchor.position + (anchor.playing ? anchor.rate * dt : 0);
}

/** Displayed playhead at `now`, seconds, never negative. */
export function playheadAt(anchor: PlayheadAnchor, now: number): number {
  const elapsed = Math.max(now - anchor.at, 0);
  const bleed = anchor.error * Math.exp(-elapsed / CORRECTION_TAU_MS);
  return Math.max(0, predicted(anchor, now) - bleed);
}

/** Anchor for a new snapshot, carrying the previous prediction as the error. */
export function nextAnchor(
  previous: PlayheadAnchor | null,
  position: number,
  rate: number,
  playing: boolean,
  now: number,
): PlayheadAnchor {
  let error = 0;
  if (previous && previous.playing) {
    const shown = playheadAt(previous, now);
    const diff = position - shown;
    // The displayed value was `shown`; report it as `position - error` at `now`.
    if (Math.abs(diff) < SNAP_THRESHOLD_S) {
      error = diff;
    }
  }
  return { position, rate, playing, at: now, error };
}
