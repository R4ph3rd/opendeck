import { describe, expect, it } from "vitest";
import {
  PeakDecodeError,
  decodePeaks,
  encodePeaks,
  levelSeconds,
  sampleRange,
  selectLevel,
  type PeakLevelData,
  type PeakSample,
} from "./peaks";

function level(fpp: number, pts: number[][], sampleRate = 48000): PeakLevelData {
  return { sampleRate, framesPerPoint: fpp, count: pts.length, points: Uint8Array.from(pts.flat()) };
}

const blank = (): PeakSample => ({ peak: 0, low: 0, mid: 0, high: 0 });

describe("decodePeaks", () => {
  it("round-trips the v1 layout", () => {
    const src = level(256, [
      [255, 10, 20, 30],
      [1, 2, 3, 4],
    ]);
    const decoded = decodePeaks(encodePeaks(src));
    expect(decoded.sampleRate).toBe(48000);
    expect(decoded.framesPerPoint).toBe(256);
    expect(decoded.count).toBe(2);
    expect(Array.from(decoded.points)).toEqual([255, 10, 20, 30, 1, 2, 3, 4]);
    expect(levelSeconds(decoded)).toBeCloseTo(512 / 48000);
  });

  it("reads the documented header bytes", () => {
    // ODPK, v1, 4 bytes per point, 44100 Hz, 64 frames, 1 point, reserved.
    const bytes = new Uint8Array([
      0x4f, 0x44, 0x50, 0x4b, 1, 0, 4, 0, 0x44, 0xac, 0, 0, 64, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 9, 8,
      7, 6,
    ]);
    const decoded = decodePeaks(bytes);
    expect(decoded.sampleRate).toBe(44100);
    expect(Array.from(decoded.points)).toEqual([9, 8, 7, 6]);
  });

  it("rejects bad magic", () => {
    const bytes = encodePeaks(level(64, [[1, 1, 1, 1]]));
    bytes[0] = 0;
    expect(() => decodePeaks(bytes)).toThrow(expect.objectContaining({ code: "bad_magic" }));
  });

  it("rejects truncated input", () => {
    expect(() => decodePeaks(new Uint8Array(10))).toThrow(PeakDecodeError);
    const bytes = encodePeaks(
      level(64, [
        [1, 1, 1, 1],
        [2, 2, 2, 2],
      ]),
    );
    expect(() => decodePeaks(bytes.subarray(0, bytes.length - 1))).toThrow(
      expect.objectContaining({ code: "truncated" }),
    );
  });

  it("rejects unknown versions and zero rates", () => {
    const v2 = encodePeaks(level(64, [[1, 1, 1, 1]]));
    v2[4] = 2;
    expect(() => decodePeaks(v2)).toThrow(expect.objectContaining({ code: "unsupported_version" }));
    const zero = encodePeaks(level(64, [[1, 1, 1, 1]]));
    zero[12] = 0;
    expect(() => decodePeaks(zero)).toThrow(expect.objectContaining({ code: "invalid" }));
  });

  it("decodes from a view with an offset", () => {
    const blob = encodePeaks(level(64, [[5, 6, 7, 8]]));
    const padded = new Uint8Array(blob.length + 3);
    padded.set(blob, 3);
    expect(Array.from(decodePeaks(padded.subarray(3)).points)).toEqual([5, 6, 7, 8]);
  });
});

describe("selectLevel", () => {
  const l0 = level(64, [[0, 0, 0, 0]]);
  const l1 = level(256, [[0, 0, 0, 0]]);
  const l2 = level(1024, [[0, 0, 0, 0]]);
  it("takes the coarsest level not wider than a pixel", () => {
    expect(selectLevel([l0, l1, l2], 300)).toBe(l1);
    expect(selectLevel([l0, l1, l2], 5000)).toBe(l2);
  });
  it("falls back to the finest and skips missing levels", () => {
    expect(selectLevel([l1, null, l2], 10)).toBe(l1);
    expect(selectLevel([null, null], 10)).toBeNull();
  });
});

describe("sampleRange", () => {
  const l = level(100, [
    [10, 10, 0, 0],
    [200, 50, 5, 1],
    [40, 60, 70, 80],
  ]);
  it("takes the max over several points", () => {
    expect(sampleRange(l, 0, 300, blank())).toEqual({ peak: 200, low: 60, mid: 70, high: 80 });
  });
  it("interpolates inside a point", () => {
    // Midway between the centres of points 0 and 1.
    const s = sampleRange(l, 90, 110, blank());
    expect(s.peak).toBeCloseTo(105, 5);
  });
  it("is zero outside the track", () => {
    expect(sampleRange(l, -500, -400, blank()).peak).toBe(0);
    expect(sampleRange(l, 1000, 1100, blank()).peak).toBe(0);
  });
  it("clips a range that hangs over the start", () => {
    expect(sampleRange(l, -200, 150, blank()).peak).toBe(200);
  });
});
