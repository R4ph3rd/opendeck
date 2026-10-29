import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { useStore } from "zustand";
import { telemetry } from "../test/fixtures";
import { framesToSeconds, selectDeck, telemetryStore } from "./telemetry";

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
