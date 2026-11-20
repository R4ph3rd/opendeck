import { describe, expect, it, vi } from "vitest";
import { PEAK_POINT_LEN, encodePeaks, type PeakLevelData } from "../viz/peaks";
import { MINI_COLUMNS, MiniPeakCache, summarizePeaks } from "./miniPeaks";

/** A level whose point `i` has peak `peaks[i]`. */
function level(peaks: number[], framesPerPoint = 1000): PeakLevelData {
  const points = new Uint8Array(peaks.length * PEAK_POINT_LEN);
  peaks.forEach((p, i) => {
    points[i * PEAK_POINT_LEN] = p;
  });
  return { sampleRate: 48_000, framesPerPoint, count: peaks.length, points };
}

const blob = (peaks: number[]) => encodePeaks(level(peaks)).buffer as ArrayBuffer;

describe("summarizePeaks", () => {
  it("takes the largest peak of each slice of the track", () => {
    const bars = summarizePeaks(level([10, 200, 30, 40, 50, 60, 70, 80]), 4);
    expect([...bars]).toEqual([200, 40, 60, 80]);
  });

  it("always returns the requested number of bars, also for a short level", () => {
    expect(summarizePeaks(level([255, 0, 128])).length).toBe(MINI_COLUMNS);
    expect(summarizePeaks(level([]), 8)).toEqual(new Uint8Array(8));
    // Fewer points than bars: each bar still reads a real point.
    const bars = summarizePeaks(level([255, 0]), 8);
    expect([...bars.slice(0, 4)].every((v) => v === 255)).toBe(true);
    expect([...bars.slice(4)].every((v) => v === 0)).toBe(true);
  });
});

describe("MiniPeakCache", () => {
  it("fetches once per track and serves later asks from the cache", async () => {
    const fetcher = vi.fn(() => Promise.resolve(blob([1, 2, 3, 4])));
    const cache = new MiniPeakCache(fetcher);
    const [a, b] = await Promise.all([cache.get(7), cache.get(7)]);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(a).toBe(b);
    expect(cache.peek(7)).toBe(a);
    await cache.get(7);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("does not remember a track without peaks, so a later ask fetches again", async () => {
    const fetcher = vi
      .fn<(track: number) => Promise<ArrayBuffer | null>>()
      .mockResolvedValueOnce(new ArrayBuffer(0))
      .mockResolvedValueOnce(blob([9, 9]));
    const cache = new MiniPeakCache(fetcher);
    expect(await cache.get(1)).toBeNull();
    expect(cache.peek(1)).toBeNull();
    expect(await cache.get(1)).not.toBeNull();
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("returns null for an unreadable blob or a failed call", async () => {
    const bad = new MiniPeakCache(() => Promise.resolve(new Uint8Array(40).buffer));
    expect(await bad.get(1)).toBeNull();
    const failing = new MiniPeakCache(() => Promise.reject(new Error("no backend")));
    expect(await failing.get(1)).toBeNull();
    expect(failing.pending).toBe(0);
  });

  it("forgets a track when asked, for a re-analysis", async () => {
    const fetcher = vi.fn(() => Promise.resolve(blob([5, 5])));
    const cache = new MiniPeakCache(fetcher);
    await cache.get(3);
    cache.forget(3);
    expect(cache.peek(3)).toBeNull();
    await cache.get(3);
    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("runs four requests at a time and serves the newest ask first", async () => {
    const releases: (() => void)[] = [];
    const order: number[] = [];
    const fetcher = vi.fn(
      (track: number) =>
        new Promise<ArrayBuffer | null>((resolve) => {
          order.push(track);
          releases.push(() => resolve(blob([track])));
        }),
    );
    const cache = new MiniPeakCache(fetcher);
    const all = [1, 2, 3, 4, 5, 6, 7].map((t) => cache.get(t));
    expect(fetcher).toHaveBeenCalledTimes(4);
    expect(order).toEqual([1, 2, 3, 4]);
    // The first four asks went straight out; of the rest, the latest goes next.
    releases[0]?.();
    await Promise.resolve();
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    expect(order[4]).toBe(7);
    for (let i = 1; i < 7; i += 1) {
      await new Promise((r) => setTimeout(r, 0));
      releases[i]?.();
    }
    await Promise.all(all);
    expect(fetcher).toHaveBeenCalledTimes(7);
  });

  it("drops the oldest waiting ask when too many pile up", async () => {
    const gate: (() => void)[] = [];
    const fetcher = vi.fn(
      () =>
        new Promise<ArrayBuffer | null>((resolve) => {
          gate.push(() => resolve(blob([1])));
        }),
    );
    const cache = new MiniPeakCache(fetcher);
    const asks = Array.from({ length: 4 + 64 + 1 }, (_, i) => cache.get(i));
    // The ask that waited longest (the queue holds 64) resolves empty at once.
    expect(await asks[4]).toBeNull();
    gate.forEach((open) => open());
  });
});
