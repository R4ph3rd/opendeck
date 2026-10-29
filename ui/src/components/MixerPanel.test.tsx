import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Profiler } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { neutralMixer } from "../test/bindingsMock";
import { telemetry } from "../test/fixtures";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { telemetryStore } = await import("../state/telemetry");
const { hydrateMixer, useMixerStore } = await import("../state/mixer");
const { MixerPanel } = await import("./MixerPanel");
const { dbToGain } = await import("../lib/mixer");

const { commands } = bindings;

beforeEach(() => {
  useMixerStore.getState().reset();
  telemetryStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(cleanup);

const slider = (name: string) => screen.getByRole("slider", { name });

function setKnob(name: string, value: number) {
  fireEvent.change(slider(name), { target: { value: String(value) } });
}

describe("MixerPanel", () => {
  it("has a strip per deck with trim, three EQ bands, filter, PFL, assign, fader and meter", () => {
    render(<MixerPanel />);
    for (const letter of ["A", "B"]) {
      const strip = within(screen.getByRole("region", { name: `Channel ${letter}` }));
      expect(strip.getByRole("slider", { name: `Trim ${letter}` })).toBeTruthy();
      for (const band of ["High", "Mid", "Low"]) {
        expect(strip.getByRole("slider", { name: `${band} EQ ${letter}` })).toBeTruthy();
      }
      expect(strip.getByRole("slider", { name: `Filter ${letter}` })).toBeTruthy();
      expect(strip.getByRole("button", { name: `Headphone cue ${letter}` })).toBeTruthy();
      expect(strip.getByRole("slider", { name: `Channel fader ${letter}` })).toBeTruthy();
      expect(strip.getByRole("meter", { name: `Level ${letter}` })).toBeTruthy();
    }
    expect(screen.getByRole("slider", { name: "Crossfader" })).toBeTruthy();
    expect(screen.getByRole("meter", { name: "Level master" })).toBeTruthy();
  });

  it("sends trim as linear gain, 0 dB at the centre", () => {
    render(<MixerPanel />);
    setKnob("Trim B", 1);
    const [deck, gain] = commands.setTrim.mock.calls[0] as unknown as [number, number];
    expect(deck).toBe(1);
    expect(gain).toBeCloseTo(dbToGain(12), 6);
    expect(screen.getAllByText("+12.0 dB").length).toBeGreaterThan(0);
  });

  it("sends each EQ band to its channel, with zero for a kill", () => {
    render(<MixerPanel />);
    setKnob("Low EQ A", 0);
    expect(commands.setEq).toHaveBeenCalledWith(0, "low", 0);
    setKnob("High EQ B", 1);
    const [deck, band, gain] = commands.setEq.mock.calls[1] as unknown as [number, string, number];
    expect([deck, band]).toEqual([1, "high"]);
    expect(gain).toBeCloseTo(dbToGain(6), 6);
    expect(useMixerStore.getState().channels[0]?.eq).toEqual([0, 0.5, 0.5]);
    expect(screen.getByText("kill")).toBeTruthy();
  });

  it("sends the filter as a bipolar amount", () => {
    render(<MixerPanel />);
    setKnob("Filter A", -0.5);
    expect(commands.setFilter).toHaveBeenCalledWith(0, -0.5);
    expect(screen.getByText("LP 50%")).toBeTruthy();
  });

  it("double-click resets a knob to its default", () => {
    render(<MixerPanel />);
    setKnob("Low EQ A", 0.1);
    commands.setEq.mockClear();
    fireEvent.doubleClick(slider("Low EQ A"));
    expect(commands.setEq).toHaveBeenCalledWith(0, "low", 1);
    expect(useMixerStore.getState().channels[0]?.eq[0]).toBe(0.5);
    fireEvent.change(slider("Filter B"), { target: { value: "0.8" } });
    fireEvent.doubleClick(slider("Filter B"));
    expect(commands.setFilter).toHaveBeenLastCalledWith(1, 0);
  });

  it("moves the channel fader from the keyboard and resets it on double-click", () => {
    render(<MixerPanel />);
    const fader = slider("Channel fader A");
    fireEvent.keyDown(fader, { key: "ArrowDown" });
    expect(commands.setChannelFader).toHaveBeenCalled();
    const [deck, position] = commands.setChannelFader.mock.calls[0] as unknown as [number, number];
    expect(deck).toBe(0);
    expect(position).toBeLessThan(1);
    fireEvent.doubleClick(fader);
    expect(commands.setChannelFader).toHaveBeenLastCalledWith(0, 1);
  });

  it("toggles PFL per channel", () => {
    render(<MixerPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Headphone cue B" }));
    expect(commands.setPfl).toHaveBeenCalledWith(1, true);
    expect(useMixerStore.getState().channels[1]?.pfl).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Headphone cue B" }));
    expect(commands.setPfl).toHaveBeenLastCalledWith(1, false);
  });

  it("assigns channels to the crossfader sides", () => {
    render(<MixerPanel />);
    fireEvent.click(screen.getByRole("radio", { name: "Assign A to A" }));
    expect(commands.setCrossfaderAssign).toHaveBeenCalledWith(0, "a");
    fireEvent.click(screen.getByRole("radio", { name: "Assign B to B" }));
    expect(commands.setCrossfaderAssign).toHaveBeenCalledWith(1, "b");
    // Clicking the active side does not send a bogus empty value.
    commands.setCrossfaderAssign.mockClear();
    fireEvent.click(screen.getByRole("radio", { name: "Assign B to B" }));
    expect(commands.setCrossfaderAssign).not.toHaveBeenCalled();
  });

  it("moves the crossfader and picks a curve", () => {
    render(<MixerPanel />);
    fireEvent.keyDown(slider("Crossfader"), { key: "ArrowRight" });
    expect(commands.setCrossfader).toHaveBeenCalled();
    const position = (commands.setCrossfader.mock.calls[0] as unknown as [number])[0];
    expect(position).toBeGreaterThan(0);
    fireEvent.doubleClick(slider("Crossfader"));
    expect(commands.setCrossfader).toHaveBeenLastCalledWith(0);

    fireEvent.click(screen.getByRole("radio", { name: "Curve Cut" }));
    expect(commands.setCrossfaderCurve).toHaveBeenCalledWith("sharp");
    fireEvent.click(screen.getByRole("radio", { name: "Curve Linear" }));
    expect(commands.setCrossfaderCurve).toHaveBeenLastCalledWith("linear");
  });

  it("controls master level, headphone level, headphone mix and split cue", () => {
    render(<MixerPanel />);
    // The default knob position is unity gain.
    fireEvent.change(slider("Master level"), { target: { value: "0" } });
    expect(commands.setMasterLevel).toHaveBeenCalledWith(0);
    fireEvent.doubleClick(slider("Master level"));
    const unity = (commands.setMasterLevel.mock.calls[1] as unknown as [number])[0];
    expect(unity).toBeCloseTo(1, 6);

    fireEvent.change(slider("Headphone level"), { target: { value: "1" } });
    const boost = (commands.setHeadphoneLevel.mock.calls[0] as unknown as [number])[0];
    expect(boost).toBeCloseTo(dbToGain(6), 6);

    fireEvent.change(slider("Headphone mix"), { target: { value: "0.5" } });
    expect(commands.setHeadphoneMix).toHaveBeenCalledWith(0.5);

    fireEvent.click(screen.getByRole("button", { name: "Split cue" }));
    expect(commands.setSplitCue).toHaveBeenCalledWith(true);
  });

  it("shows what the backend reports at startup", async () => {
    const mixer = neutralMixer();
    mixer.channels = mixer.channels.map((c, i) =>
      i === 1 ? { ...c, fader: 0.4, pfl: true, crossfader_assign: "b" } : c,
    );
    mixer.crossfader = -0.5;
    mixer.crossfader_curve = "sharp";
    mixer.split_cue = true;
    commands.getMixerState.mockResolvedValueOnce({ status: "ok", data: mixer } as never);
    render(<MixerPanel />);
    await act(() => hydrateMixer());
    expect((slider("Channel fader B") as HTMLElement).getAttribute("aria-valuenow")).toBe("0.4");
    expect(screen.getByRole("button", { name: "Headphone cue B" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(screen.getByRole("radio", { name: "Curve Cut" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("button", { name: "Split cue" }).getAttribute("aria-pressed")).toBe("true");
    expect(useMixerStore.getState().crossfader).toBe(-0.5);
    // Unity gains read back as the centre of the trim and EQ knobs.
    expect(useMixerStore.getState().channels[0]?.trim).toBeCloseTo(0.5, 6);
    expect(useMixerStore.getState().master).toBeCloseTo(0.75, 6);
  });

  it("does not re-render on a telemetry tick", () => {
    let commits = 0;
    render(
      <Profiler id="mixer" onRender={() => commits++}>
        <MixerPanel />
      </Profiler>,
    );
    const before = commits;
    for (let sequence = 1; sequence < 30; sequence++) {
      act(() => telemetryStore.getState().push(telemetry(sequence)));
    }
    expect(commits).toBe(before);
  });
});
