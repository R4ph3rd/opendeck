import { describe, expect, it, vi } from "vitest";
import { PeakCache, OVERVIEW_LEVEL } from "./peakCache";
import { encodePeaks } from "./peaks";

const blob = (fpp: number): ArrayBuffer => {
  const bytes = encodePeaks({
    sampleRate: 44100,
    framesPerPoint: fpp,
    count: 1,
    points: Uint8Array.from([1, 2, 3, 4]),
  });
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
};

describe("PeakCache", () => {
  it("decodes once and serves later calls from memory", async () => {
    const fetcher = vi.fn((_t: number, level: number) => Promise.resolve(blob(64 << level)));
    const cache = new PeakCache(fetcher);
    const [a, b] = await Promise.all([cache.level(1, 0), cache.level(1, 0)]);
    expect(a).toBe(b);
    await cache.level(1, 0);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("does not cache an empty payload, so analysis can finish later", async () => {
    const fetcher = vi
      .fn<(t: number, l: number) => Promise<ArrayBuffer>>()
      .mockResolvedValueOnce(new ArrayBuffer(0))
      .mockResolvedValue(blob(64));
    const cache = new PeakCache(fetcher);
    expect(await cache.level(2, 0)).toBeNull();
    expect(await cache.level(2, 0)).not.toBeNull();
  });

  it("treats garbage as missing", async () => {
    const cache = new PeakCache(() => Promise.resolve(new Uint8Array(40).buffer));
    expect(await cache.level(3, 0)).toBeNull();
  });

  it("loads every level of a track and reports progress", async () => {
    const fetcher = vi.fn((_t: number, level: number) => Promise.resolve(blob(level + 1)));
    const cache = new PeakCache(fetcher);
    const progress = vi.fn();
    const peaks = await cache.track(7, progress);
    expect(peaks.overview?.framesPerPoint).toBe(OVERVIEW_LEVEL + 1);
    expect(peaks.detail.every((l) => l !== null)).toBe(true);
    expect(progress).toHaveBeenCalledTimes(4);
  });

  it("evicts the least recently used track", async () => {
    const cache = new PeakCache(() => Promise.resolve(blob(64)));
    for (let t = 1; t <= 8; t += 1) await cache.level(t, 0);
    expect(cache.peek(1, 0)).toBeNull();
    expect(cache.peek(8, 0)).not.toBeNull();
  });
});
