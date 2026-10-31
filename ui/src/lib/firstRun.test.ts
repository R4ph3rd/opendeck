import { describe, expect, it } from "vitest";
import type { AudioDeviceInfo, AudioSettings, MidiMonitorEntryDto } from "../ipc/bindings";
import {
  FLX4_HEADPHONES,
  FLX4_MASTER,
  findFlx4Device,
  findFlx4Port,
  flx4Settings,
  isFlx4Name,
  isPlayPress,
} from "./firstRun";

const device = (name: string): AudioDeviceInfo => ({
  name,
  is_default: false,
  max_channels: 4,
  default_sample_rate: 48_000,
  sample_rate_ranges: [],
  buffer_frames: null,
});

const entry = (bytes: number[], deck: number, action = "play_pause"): MidiMonitorEntryDto => ({
  ts_us: 0,
  direction: "in",
  bytes,
  text: "",
  label: {
    kind: "control",
    id: "deck1.play",
    label: null,
    action: { action, deck } as never,
    verify: null,
  },
});

describe("first run helpers", () => {
  it("detects the FLX4 by name", () => {
    expect(isFlx4Name("DDJ-FLX4")).toBe(true);
    expect(isFlx4Name("Pioneer DDJ-FLX4 MIDI 1")).toBe(true);
    expect(isFlx4Name("Speakers")).toBe(false);
    expect(findFlx4Device([device("Speakers"), device("DDJ-FLX4")])?.name).toBe("DDJ-FLX4");
    expect(findFlx4Device([device("Speakers")])).toBeUndefined();
    expect(
      findFlx4Port([
        { name: "DDJ-FLX4", has_input: false, has_output: true, matches_mapping: true },
        { name: "DDJ-FLX4 MIDI", has_input: true, has_output: true, matches_mapping: true },
      ])?.name,
    ).toBe("DDJ-FLX4 MIDI");
  });

  it("routes master to 1/2 and headphones to 3/4", () => {
    const base: AudioSettings = {
      host: null,
      device: null,
      sample_rate: 44_100,
      buffer_frames: null,
      master: { left: 4, right: 5 },
      headphones: null,
    };
    expect(flx4Settings(base, device("DDJ-FLX4"))).toEqual({
      ...base,
      device: "DDJ-FLX4",
      master: FLX4_MASTER,
      headphones: FLX4_HEADPHONES,
    });
  });

  it("recognises a PLAY press on the requested deck only", () => {
    expect(isPlayPress(entry([0x90, 0x0b, 0x7f], 0), 0)).toBe(true);
    expect(isPlayPress(entry([0x90, 0x0b, 0x00], 0), 0)).toBe(false);
    expect(isPlayPress(entry([0x91, 0x0b, 0x7f], 1), 0)).toBe(false);
    expect(isPlayPress(entry([0x90, 0x0c, 0x7f], 0, "cue"), 0)).toBe(false);
  });
});
