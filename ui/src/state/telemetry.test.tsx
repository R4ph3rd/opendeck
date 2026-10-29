import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useStore } from "zustand";
import { telemetry, telemetryFor } from "../test/fixtures";
import {
  framesToSeconds,
  isClipping,
  meterFraction,
  selectBpm,
  selectDeck,
  selectIsLeader,
  selectLeader,
  selectLoopActive,
  selectSyncMode,
  telemetryStore,
} from "./telemetry";

beforeEach(() => telemetryStore.getState().reset());
afterEach(cleanup);

describe("telemetry store", () => {
  it("starts empty and keeps the newest snapshot", () => {
    expect(telemetryStore.getState().telemetry).toBeNull();
    telemetryStore.getState().push(telemetry(1));
    telemetryStore.getState().push(telemetry(2, { playing: true }));
    expect(telemetryStore.getState().telemetry?.sequence).toBe(2);
  });

  it("ignores a snapshot older than the one it holds", () => {
    telemetryStore.getState().push(telemetry(5, { position: 500 }));
    telemetryStore.getState().push(telemetry(4, { position: 400 }));
    expect(telemetryStore.getState().telemetry?.decks[0].position).toBe(500);
  });

  it("selects one deck and tolerates an empty store", () => {
    expect(selectDeck(telemetryStore.getState(), 0)).toBeNull();
    telemetryStore.getState().push(telemetry(1, { loaded: true }));
    expect(selectDeck(telemetryStore.getState(), 0)?.loaded).toBe(true);
    expect(selectDeck(telemetryStore.getState(), 9)).toBeNull();
  });

  it("converts frames to seconds and guards a zero rate", () => {
    expect(framesToSeconds(96_000, 48_000)).toBe(2);
    expect(framesToSeconds(10, 0)).toBe(0);
  });

  it("does not re-render a coarse subscriber for every tick", () => {
    let renders = 0;
    function Probe() {
      const playing = useStore(telemetryStore, (s) => s.telemetry?.decks[0].playing ?? false);
      renders += 1;
      return <p>{playing ? "playing" : "paused"}</p>;
    }
    const { getByText } = render(<Probe />);
    const afterMount = renders;
    act(() => {
      for (let sequence = 1; sequence <= 60; sequence++) {
        telemetryStore.getState().push(telemetry(sequence, { position: sequence * 800 }));
      }
    });
    expect(renders).toBe(afterMount);
    act(() => telemetryStore.getState().push(telemetry(61, { playing: true })));
    expect(getByText("playing")).toBeTruthy();
    expect(renders).toBe(afterMount + 1);
  });
});

describe("meters, sync state and loop state", () => {
  it("maps a peak to a meter fraction between -60 dBFS and full scale", () => {
    expect(meterFraction(null)).toBe(0);
    expect(meterFraction(0)).toBe(0);
    expect(meterFraction(0.0005)).toBe(0);
    expect(meterFraction(1)).toBe(1);
    expect(meterFraction(2)).toBe(1);
    expect(meterFraction(0.5)).toBeCloseTo((20 * Math.log10(0.5) + 60) / 60, 9);
    expect(meterFraction(0.1)).toBeLessThan(meterFraction(0.5));
  });

  it("flags clipping at full scale", () => {
    expect(isClipping(0.99)).toBe(false);
    expect(isClipping(1)).toBe(true);
    expect(isClipping(null)).toBe(false);
  });

  it("selects the loop, sync, leader and bpm of each deck", () => {
    const state = () => telemetryStore.getState();
    expect(selectLoopActive(state(), 1)).toBe(false);
    expect(selectLeader(state())).toBeNull();
    expect(selectIsLeader(state(), 1)).toBe(false);
    expect(selectSyncMode(state(), 1)).toBeNull();
    expect(selectBpm(state(), 1)).toBeNull();
    const loop = { start: 0, end: 96_000 };
    state().push(
      telemetryFor(1, 1, { bpm: 126, sync_mode: "beat", loop_region: loop, loop_active: true }, 1),
    );
    expect(selectLoopActive(state(), 1)).toBe(true);
    expect(selectLoopActive(state(), 0)).toBe(false);
    expect(selectLeader(state())).toBe(1);
    expect(selectIsLeader(state(), 1)).toBe(true);
    expect(selectIsLeader(state(), 0)).toBe(false);
    expect(selectSyncMode(state(), 1)).toBe("beat");
    expect(selectSyncMode(state(), 0)).toBe("off");
    expect(selectBpm(state(), 1)).toBe(126);
    expect(selectBpm(state(), 0)).toBeNull();
    // A stored loop that is not playing does not light the loop.
    state().push(telemetryFor(2, 1, { loop_region: loop, loop_active: false }));
    expect(selectLoopActive(state(), 1)).toBe(false);
  });

  it("re-renders a loop selector only when the flag changes, not per tick", () => {
    let renders = 0;
    function Probe() {
      renders++;
      const looping = useStore(telemetryStore, (s) => selectLoopActive(s, 1));
      return <span>{String(looping)}</span>;
    }
    render(<Probe />);
    const initial = renders;
    for (let sequence = 1; sequence <= 20; sequence++) {
      act(() => telemetryStore.getState().push(telemetry(sequence)));
    }
    expect(renders).toBe(initial);
    act(() => {
      telemetryStore.getState().push(telemetryFor(30, 1, { loop_region: { start: 0, end: 48_000 }, loop_active: true }));
    });
    expect(renders).toBe(initial + 1);
  });
});
