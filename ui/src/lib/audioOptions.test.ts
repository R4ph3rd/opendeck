import { describe, expect, it } from "vitest";
import {
  gainDbLabel,
  hostUnavailableReason,
  inputKey,
  inputLabel,
  micOptions,
  parseInputKey,
} from "./audioOptions";

const device = (max_channels: number) => ({
  name: "Interface",
  is_default: true,
  max_channels,
  default_sample_rate: 48_000,
  sample_rate_ranges: [],
  buffer_frames: null,
});

describe("audio options", () => {
  it("explains unavailable hosts", () => {
    const host = { name: "x", is_default: false };
    expect(hostUnavailableReason({ ...host, kind: "asio", available: false })).toBe(
      "build with the asio feature",
    );
    expect(hostUnavailableReason({ ...host, kind: "jack", available: false })).toBe(
      "build with the jack feature",
    );
    expect(hostUnavailableReason({ ...host, kind: "alsa", available: true })).toBeNull();
  });

  it("offers mono channels then pairs for the mic", () => {
    const options = micOptions(device(4));
    expect(options.map(inputLabel)).toEqual(["In 1", "In 2", "In 3", "In 4", "In 1/2", "In 3/4"]);
    for (const option of options) {
      expect(parseInputKey(inputKey(option))).toEqual(option);
    }
    expect(parseInputKey("x")).toBeNull();
  });

  it("formats gains in dB", () => {
    expect(gainDbLabel(1)).toBe("0.0 dB");
    expect(gainDbLabel(0)).toBe("-inf dB");
    expect(gainDbLabel(0.5)).toBe("-6.0 dB");
    expect(gainDbLabel(1.5)).toBe("+3.5 dB");
  });
});
