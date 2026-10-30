import { commands } from "../ipc/bindings";
import { decodePeaks, type PeakLevelData } from "./peaks";

/** `get_track_peaks` levels: 0 to 2 are detail (64, 256, 1024 frames per point), 3 is the overview. */
export const DETAIL_LEVELS = [0, 1, 2] as const;
export const OVERVIEW_LEVEL = 3;

/** Peaks of one track as loaded so far. `null` entries are missing or not analysed yet. */
export interface TrackPeaks {
  detail: (PeakLevelData | null)[];
  overview: PeakLevelData | null;
}

export const EMPTY_PEAKS: TrackPeaks = { detail: [null, null, null], overview: null };

/** Tracks kept decoded. A level is a few MB for a long track. */
const MAX_TRACKS = 6;

type Fetcher = (track: number, level: number) => Promise<ArrayBuffer | null>;

/** Production fetcher: the raw command, an `ArrayBuffer` at runtime. */
const fetchFromBackend: Fetcher = async (track, level) => {
  const result = await commands.getTrackPeaks(track, level);
  if (result.status !== "ok") {
    return null;
  }
  // The binding types the payload as `number[]`; on the wire it is an ArrayBuffer (ADR 0004).
  const bytes = result.data as unknown as ArrayBuffer | number[];
  return Array.isArray(bytes) ? Uint8Array.from(bytes).buffer : bytes;
};

/**
 * Per-track cache of decoded peak levels. Concurrent requests share one fetch.
 * Empty payloads (not analysed yet) and decode failures are not cached, so a
 * later call retries.
 */
export class PeakCache {
  private levels = new Map<string, PeakLevelData>();
  private inflight = new Map<string, Promise<PeakLevelData | null>>();
  private order: number[] = [];

  constructor(private readonly fetcher: Fetcher = fetchFromBackend) {}

  /** Decoded level if already cached. */
  peek(track: number, level: number): PeakLevelData | null {
    return this.levels.get(`${track}:${level}`) ?? null;
  }

  async level(track: number, level: number): Promise<PeakLevelData | null> {
    const key = `${track}:${level}`;
    const hit = this.levels.get(key);
    if (hit) {
      this.touch(track);
      return hit;
    }
    const pending = this.inflight.get(key);
    if (pending) {
      return pending;
    }
    const request = (async () => {
      try {
        const buffer = await this.fetcher(track, level);
        if (!buffer || buffer.byteLength === 0) {
          return null;
        }
        const decoded = decodePeaks(buffer);
        this.levels.set(key, decoded);
        this.touch(track);
        return decoded;
      } catch {
        return null;
      } finally {
        this.inflight.delete(key);
      }
    })();
    this.inflight.set(key, request);
    return request;
  }

  /** Everything cached for a track right now, without fetching. */
  snapshot(track: number): TrackPeaks {
    return {
      detail: DETAIL_LEVELS.map((l) => this.peek(track, l)),
      overview: this.peek(track, OVERVIEW_LEVEL),
    };
  }

  /** Fetches the overview first (small, shows at once), then the detail levels. */
  async track(track: number, onProgress?: (peaks: TrackPeaks) => void): Promise<TrackPeaks> {
    const jobs = [OVERVIEW_LEVEL, 2, 1, 0].map(async (level) => {
      await this.level(track, level);
      onProgress?.(this.snapshot(track));
    });
    await Promise.all(jobs);
    return this.snapshot(track);
  }

  clear(): void {
    this.levels.clear();
    this.order = [];
  }

  private touch(track: number): void {
    this.order = this.order.filter((t) => t !== track);
    this.order.push(track);
    while (this.order.length > MAX_TRACKS) {
      const evicted = this.order.shift();
      for (const key of [...this.levels.keys()]) {
        if (key.startsWith(`${evicted}:`)) {
          this.levels.delete(key);
        }
      }
    }
  }
}

/** Shared by all decks. */
export const peakCache = new PeakCache();
