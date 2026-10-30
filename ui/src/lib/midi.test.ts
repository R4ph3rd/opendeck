import { describe, expect, it } from "vitest";
import type { MidiMappingRowDto, MidiMonitorEntryDto } from "../ipc/bindings";
import {
  NO_MONITOR_FILTER,
  decodeLabel,
  describeAction,
  entryMatches,
  formatBinding,
  formatBytes,
  formatTime,
  rowMatches,
  sameBinding,
} from "./midi";

const row = (patch: Partial<MidiMappingRowDto> = {}): MidiMappingRowDto => ({
  id: "deck1.play",
  label: "PLAY/PAUSE",
  source_ref: "1-1",
  binding: { type: "note", channel: 1, number: 11 },
  action: { action: "play_pause", deck: 0 },
  layer: "base",
  value_kind: "button",
  verify: null,
  overridden: false,
  ...patch,
});

const entry = (patch: Partial<MidiMonitorEntryDto> = {}): MidiMonitorEntryDto => ({
  ts_us: 1_500_000,
  direction: "in",
  bytes: [0x90, 0x0b, 0x7f],
  text: "Note On ch1 0x0B vel 127",
  label: {
    kind: "control",
    id: "deck1.play",
    label: "PLAY/PAUSE",
    action: { action: "play_pause", deck: 0 },
    verify: null,
  },
  ...patch,
});

describe("formatting", () => {
  it("prints bytes and bindings the way the message list does", () => {
    expect(formatBytes([0x90, 0x0b, 0x7f, 0])).toBe("90 0B 7F 00");
    expect(formatBinding({ type: "note", channel: 1, number: 11 })).toBe("Note 0x0B ch1");
    expect(formatBinding({ type: "cc", channel: 7, number: 64 })).toBe("CC 0x40 ch7");
    expect(formatBinding({ type: "cc14", channel: 1, msb: 7, lsb: 39 })).toBe("CC14 0x07/0x27 ch1");
  });

  it("compares bindings by value", () => {
    const a = { type: "note", channel: 1, number: 11 } as const;
    expect(sameBinding(a, { ...a })).toBe(true);
    expect(sameBinding(a, { ...a, number: 12 })).toBe(false);
  });

  it("formats time relative to the first message", () => {
    expect(formatTime(2_500_000, 1_000_000)).toBe("1.500 s");
    expect(formatTime(500_000, 1_000_000)).toBe("0.000 s");
  });
});

describe("describeAction", () => {
  it("names the deck, the action and its parameters", () => {
    expect(describeAction({ action: "play_pause", deck: 1 })).toBe("Deck 2 Play/Pause");
    expect(describeAction({ action: "eq", deck: 0, band: "high" })).toBe("Deck 1 EQ high");
    expect(describeAction({ action: "pad", deck: 0, mode: "beat_loop", index: 2, shifted: true })).toBe(
      "Deck 1 Pad Beat loop, 3, shift",
    );
    expect(describeAction({ action: "crossfader" })).toBe("Crossfader");
    expect(describeAction({ action: "beat_jump", deck: 0, beats: -4 })).toBe("Deck 1 Beat jump -4 beats");
  });
});

describe("mapping search", () => {
  it("matches id, message, action and the verify flag, ignoring case", () => {
    expect(rowMatches(row(), "")).toBe(true);
    expect(rowMatches(row(), "DECK1.PLAY")).toBe(true);
    expect(rowMatches(row(), "0x0b")).toBe(true);
    expect(rowMatches(row(), "play/pause")).toBe(true);
    expect(rowMatches(row(), "tempo")).toBe(false);
    expect(rowMatches(row({ verify: "Q07 channel" }), "verify")).toBe(true);
    expect(rowMatches(row(), "verify")).toBe(false);
  });
});

describe("monitor decoding", () => {
  it("shows the mapped control and its action", () => {
    expect(decodeLabel(entry())).toEqual({
      text: "deck1.play: Deck 1 Play/Pause",
      verify: null,
    });
  });

  it("carries the verify note of a control", () => {
    const e = entry({
      label: { kind: "control", id: "deck1.tempo", label: null, action: null, verify: "Q07 x" },
    });
    expect(decodeLabel(e)).toEqual({ text: "deck1.tempo", verify: "Q07 x" });
  });

  it("explains messages that have no control", () => {
    expect(decodeLabel(entry({ label: { kind: "unmapped" } })).text).toBe("not in the mapping");
    expect(
      decodeLabel(entry({ label: { kind: "known_unmapped", source_ref: "3-12", reason: "no mic" } }))
        .text,
    ).toBe("unmapped 3-12: no mic");
    expect(decodeLabel(entry({ label: { kind: "output" } })).text).toBe("");
    expect(decodeLabel(entry({ label: { kind: "modifier", id: "shift_deck1", verify: null } })).text).toBe(
      "shift_deck1 (held)",
    );
  });

  it("filters by direction and by bytes, text or control", () => {
    const out = entry({ direction: "out", label: { kind: "output" }, bytes: [0x90, 0x0b, 0x00] });
    expect(entryMatches(entry(), NO_MONITOR_FILTER)).toBe(true);
    expect(entryMatches(out, { ...NO_MONITOR_FILTER, showOut: false })).toBe(false);
    expect(entryMatches(entry(), { ...NO_MONITOR_FILTER, showIn: false })).toBe(false);
    expect(entryMatches(entry(), { ...NO_MONITOR_FILTER, text: "90 0b" })).toBe(true);
    expect(entryMatches(entry(), { ...NO_MONITOR_FILTER, text: "play/pause" })).toBe(true);
    expect(entryMatches(entry(), { ...NO_MONITOR_FILTER, text: "sampler" })).toBe(false);
  });
});
