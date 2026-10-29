import { describe, expect, it } from "vitest";
import {
  bufferOptions,
  channelPairs,
  pairKey,
  pairLabel,
  parsePairKey,
  sampleRateOptions,
} from "./audioOptions";
import { bufferLatencyMs, formatRemaining, formatTempo, formatTime } from "./format";

describe("format", () => {
  it("formats times", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(65.9)).toBe("1:05");
    expect(formatTime(3725)).toBe("1:02:05");
    expect(formatTime(-3)).toBe("0:00");
    expect(formatTime(Number.NaN)).toBe("0:00");
    expect(formatRemaining(61)).toBe("-1:01");
  });

  it("formats the tempo change with a sign", () => {
    expect(formatTempo(0, 0.1)).toBe("0.0%");
    expect(formatTempo(0.5, 0.1)).toBe("+5.0%");
    expect(formatTempo(-1, 0.06)).toBe("-6.0%");
    expect(formatTempo(0.0001, 0.1)).toBe("0.0%");
  });

  it("computes buffer latency", () => {
    expect(bufferLatencyMs(480, 48_000)).toBeCloseTo(10);
    expect(bufferLatencyMs(480, 0)).toBe(0);
  });
});

describe("audio options", () => {
  const device = {
    name: "d",
    is_default: true,
    max_channels: 6,
    default_sample_rate: 48_000,
    sample_rate_ranges: [[44_100, 96_000]] as [number, number][],
    buffer_frames: [128, 1024] as [number, number],
  };

  it("keeps only rates and buffers the device supports", () => {
    expect(sampleRateOptions(device)).toEqual([44_100, 48_000, 88_200, 96_000]);
    expect(bufferOptions(device)).toEqual([128, 256, 512, 1024]);
    expect(sampleRateOptions(undefined)).toEqual([44_100, 48_000]);
    expect(bufferOptions(undefined)).toContain(64);
  });

  it("lists channel pairs", () => {
    expect(channelPairs(device).map(pairLabel)).toEqual(["1/2", "3/4", "5/6"]);
    expect(channelPairs(undefined).map(pairLabel)).toEqual(["1/2", "3/4"]);
    expect(channelPairs({ ...device, max_channels: 1 }).map(pairLabel)).toEqual(["1/2"]);
  });

  it("round-trips pair keys", () => {
    const pair = { left: 2, right: 3 };
    expect(parsePairKey(pairKey(pair))).toEqual(pair);
    expect(parsePairKey("x")).toBeNull();
  });
});
