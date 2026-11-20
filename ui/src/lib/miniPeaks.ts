import { commands } from "../ipc/bindings";
import { OVERVIEW_LEVEL } from "../viz/peakCache";
import { PEAK_POINT_LEN, decodePeaks, sampleRange, type PeakLevelData } from "../viz/peaks";

/** Bars in a collection mini waveform. */
export const MINI_COLUMNS = 64;

/** Tracks whose summary is kept. A summary is `MINI_COLUMNS` bytes. */
const MAX_CACHED = 4000;

/** Longest a fetch may stay queued behind others before it is dropped, entries. */
const MAX_QUEUED = 64;

/** Requests in flight at once. */
const MAX_IN_FLIGHT = 4;

/**
 * Reduces a peak level to `columns` bars (the largest peak in each slice of
 * the track), `0..=255`. The result is what the collection draws.
 */
export function summarizePeaks(level: PeakLevelData, columns = MINI_COLUMNS): Uint8Array {
  const out = new Uint8Array(columns);
  const total = level.count * level.framesPerPoint;
  if (total <= 0 || columns <= 0) {
    return out;
  }
  const scratch = { peak: 0, low: 0, mid: 0, high: 0 };
  for (let x = 0; x < columns; x += 1) {
    const a = Math.floor((x / columns) * total);
    const b = Math.max(a + 1, Math.floor(((x + 1) / columns) * total));
    out[x] = sampleRange(level, a, b, scratch).peak;
  }
  return out;
}

/** The peak blob of the lowest-resolution level; `null` when the track has none (yet). */
export type PeakFetcher = (track: number) => Promise<ArrayBuffer | null>;

/**
 * The raw command: the payload is an `ArrayBuffer` on the wire, not a JSON
 * array (ADR 0004), although the generated binding types it as `number[]`.
 */
const fetchOverview: PeakFetcher = async (track) => {
  const result = await commands.getTrackPeaks(track, OVERVIEW_LEVEL);
  if (result.status !== "ok") {
    return null;
  }
  const bytes = result.data as unknown as ArrayBuffer | number[];
  return Array.isArray(bytes) ? Uint8Array.from(bytes).buffer : bytes;
};

interface Waiting {
  track: number;
  resolve: (bars: Uint8Array | null) => void;
}

/**
 * Cache of mini waveforms for the collection. A row asks for its track when
 * it is on screen (the table is virtualised, so that is only the visible
 * rows plus a small margin); concurrent asks share one request and at most
 * `MAX_IN_FLIGHT` run at once, so a fast scroll cannot flood the backend. A
 * track with no peaks yet is not remembered: the next ask fetches again.
 */
export class MiniPeakCache {
  private bars = new Map<number, Uint8Array>();
  private inflight = new Map<number, Promise<Uint8Array | null>>();
  private queue: Waiting[] = [];
  private running = 0;

  constructor(private readonly fetcher: PeakFetcher = fetchOverview) {}

  /** The bars if already cached. Marks the track as recently used. */
  peek(track: number): Uint8Array | null {
    const hit = this.bars.get(track);
    if (hit) {
      this.bars.delete(track);
      this.bars.set(track, hit);
    }
    return hit ?? null;
  }

  /** Requests currently running or waiting. */
  get pending(): number {
    return this.running + this.queue.length;
  }

  /** The bars of `track`, fetched on first use; `null` when it has no peaks. */
  get(track: number): Promise<Uint8Array | null> {
    const hit = this.peek(track);
    if (hit) {
      return Promise.resolve(hit);
    }
    const pending = this.inflight.get(track);
    if (pending) {
      return pending;
    }
    const request = new Promise<Uint8Array | null>((resolve) => {
      // A full queue drops its oldest entry: that row has scrolled away by now.
      if (this.queue.length >= MAX_QUEUED) {
        const dropped = this.queue.shift();
        dropped?.resolve(null);
        if (dropped) this.inflight.delete(dropped.track);
      }
      this.queue.push({ track, resolve });
      this.pump();
    });
    this.inflight.set(track, request);
    return request;
  }

  /** Drops the bars of `track` (it was analysed again), so the next ask fetches new ones. */
  forget(track: number): void {
    this.bars.delete(track);
  }

  clear(): void {
    this.bars.clear();
  }

  private pump(): void {
    while (this.running < MAX_IN_FLIGHT) {
      const next = this.queue.pop();
      if (!next) {
        return;
      }
      this.running += 1;
      void this.run(next);
    }
  }

  private async run({ track, resolve }: Waiting): Promise<void> {
    let bars: Uint8Array | null = null;
    try {
      const buffer = await this.fetcher(track);
      if (buffer && buffer.byteLength >= PEAK_POINT_LEN) {
        bars = summarizePeaks(decodePeaks(buffer));
        this.bars.set(track, bars);
        while (this.bars.size > MAX_CACHED) {
          const oldest = this.bars.keys().next();
          if (oldest.done) break;
          this.bars.delete(oldest.value);
        }
      }
    } catch {
      // No backend, or a blob this build cannot read: no waveform.
    } finally {
      this.running -= 1;
      this.inflight.delete(track);
    }
    resolve(bars);
    this.pump();
  }
}

/** Shared by every collection table. */
export const miniPeaks = new MiniPeakCache();
