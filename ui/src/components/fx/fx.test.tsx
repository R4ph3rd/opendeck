import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../../test/bindingsMock";
import { fxEffects, neutralFxState, neutralMixer } from "../../test/bindingsMock";
import { telemetry } from "../../test/fixtures";

vi.mock("../../ipc/bindings", async () =>
  (await import("../../test/bindingsMock")).createBindingsMock(),
);

const bindings = (await import("../../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { telemetryStore } = await import("../../state/telemetry");
const { useFxStore, hydrateFx } = await import("../../state/fx");
const { useMixerStore } = await import("../../state/mixer");
const { FxPanel } = await import("./FxPanel");
const { PadFxEditor } = await import("./PadFxEditor");
const { ChannelStrip } = await import("../ChannelStrip");

const { commands } = bindings;
const mock = commands as unknown as Record<string, ReturnType<typeof vi.fn>>;

beforeEach(async () => {
  useFxStore.getState().reset();
  useMixerStore.getState().reset();
  telemetryStore.getState().reset();
  vi.clearAllMocks();
  await hydrateFx();
});

afterEach(cleanup);

const slider = (name: string) => screen.getByRole("slider", { name });
const applied = () => (mock.fxApply?.mock.calls ?? []).map((c) => c[0] as { kind: string });

function setKnob(name: string, value: number) {
  fireEvent.change(slider(name), { target: { value: String(value) } });
}

describe("FxPanel", () => {
  it("shows two Beat FX units with effect, channel, mode and BPM controls", () => {
    render(<FxPanel />);
    for (const n of [1, 2]) {
      const unit = within(screen.getByRole("region", { name: `Beat FX ${n}` }));
      expect(unit.getByRole("combobox", { name: `Beat FX ${n} slot 1 effect` })).toBeTruthy();
      for (const ch of ["1", "2", "1&2"]) {
        expect(unit.getByRole("radio", { name: `Beat FX ${n} channel ${ch}` })).toBeTruthy();
      }
      expect(unit.getByRole("button", { name: `Beat FX ${n} release` })).toBeTruthy();
      expect(unit.getByRole("button", { name: `Beat FX ${n} tap tempo` })).toBeTruthy();
      expect(unit.getByRole("radio", { name: `Beat FX ${n} multi mode` })).toBeTruthy();
    }
    // Unit 1 is on channel 1 and shows its effect and division.
    const first = within(screen.getByRole("region", { name: "Beat FX 1" }));
    expect(first.getByText("Echo")).toBeTruthy();
    expect(first.getByRole("status", { name: "Beat FX 1 division" }).textContent).toBe("3/4");
    expect(first.getByRole("radio", { name: "Beat FX 1 channel 1" }).getAttribute("aria-checked")).toBe("true");
  });

  it("sends channel assignment, mode, ON/OFF, release and beat steps", () => {
    render(<FxPanel />);
    const unit = within(screen.getByRole("region", { name: "Beat FX 1" }));
    fireEvent.click(unit.getByRole("radio", { name: "Beat FX 1 channel 1&2" }));
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "set_assign", unit: 0, assign: "both" });
    fireEvent.click(unit.getByRole("radio", { name: "Beat FX 1 single mode" }));
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "set_mode", unit: 0, mode: "single" });
    fireEvent.click(unit.getByRole("button", { name: "Beat FX 1 on/off" }));
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "set_unit_on", unit: 0, on: true });
    fireEvent.click(unit.getByRole("button", { name: "Beat FX 1 release" }));
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "release", unit: 0 });
    fireEvent.click(unit.getByRole("button", { name: "Beat FX 1 beat right" }));
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "step_division",
      unit: { target: "beat", unit: 0 },
      direction: 1,
    });
    fireEvent.click(unit.getByRole("button", { name: "Beat FX 1 beat left" }));
    expect(applied().filter((o) => o.kind === "step_division")).toHaveLength(2);
  });

  it("lights ON/OFF from the engine report of the assigned channel", () => {
    render(<FxPanel />);
    const button = () => screen.getByRole("button", { name: "Beat FX 1 on/off" });
    act(() => telemetryStore.getState().push(telemetry(1)));
    expect(button().getAttribute("aria-pressed")).toBe("false");
    const on = telemetry(2);
    on.fx_decks[0] = { enabled: true, active: true, level: 0.2 };
    act(() => telemetryStore.getState().push(on));
    expect(button().getAttribute("aria-pressed")).toBe("true");
    // Unit 2 sits on channel 2, which stays off.
    expect(screen.getByRole("button", { name: "Beat FX 2 on/off" }).getAttribute("aria-pressed")).toBe(
      "false",
    );
  });

  it("drives the primary parameter from the level knob and shows it at once", () => {
    render(<FxPanel />);
    setKnob("Beat FX 1 slot 1 level", 0.8);
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "set_level",
      slot: { unit: { target: "beat", unit: 0 }, slot: 0 },
      value: 0.8,
    });
    // Decay is 0..98 %, so 0.8 reads 78 %, and the model already holds it.
    expect(screen.getAllByText("78%").length).toBeGreaterThan(0);
    expect(useFxStore.getState().state?.units[0]?.slots[0]?.params[1]).toBe(0.8);
  });

  it("taps tempo and reads the tapped BPM", () => {
    render(<FxPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Beat FX 1 tap tempo" }));
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "tap", unit: 0 });
    expect(screen.getByRole("status", { name: "Beat FX 1 BPM readout" }).textContent).toBe("-");
    const state = neutralFxState();
    const unit = state.units[0];
    if (unit) {
      unit.bpm_source = "tap";
      unit.tap_bpm = 123.4;
    }
    act(() => useFxStore.getState().setState(state as never));
    expect(screen.getByRole("status", { name: "Beat FX 1 BPM readout" }).textContent).toBe("123.4");
    fireEvent.click(screen.getByRole("radio", { name: "Beat FX 1 BPM auto" }));
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "set_bpm_source",
      unit: 0,
      source: "auto",
    });
  });

  it("shows every parameter with its unit in Single mode", () => {
    render(<FxPanel />);
    const state = neutralFxState();
    const unit = state.units[0];
    if (unit) {
      unit.mode = "single";
    }
    act(() => useFxStore.getState().setState(state as never));
    const region = within(screen.getByRole("region", { name: "Beat FX 1" }));
    expect(region.getAllByRole("combobox")).toHaveLength(1);
    expect(region.getByRole("slider", { name: "Beat FX 1 Echo Time" })).toBeTruthy();
    expect(region.getByRole("slider", { name: "Beat FX 1 Echo Decay" })).toBeTruthy();
    expect(region.getByRole("slider", { name: "Beat FX 1 Echo Damping" })).toBeTruthy();
    expect(region.getByRole("slider", { name: "Beat FX 1 Echo mix" })).toBeTruthy();
    expect(region.getByRole("slider", { name: "Beat FX 1 Echo Decay" }).getAttribute("aria-valuetext")).toBe(
      "49%",
    );
    const damping = region.getByRole("slider", { name: "Beat FX 1 Echo Damping" });
    expect(damping.getAttribute("aria-valuetext")).toMatch(/kHz|Hz/);
    setKnob("Beat FX 1 Echo Time", 0.5);
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "set_param",
      slot: { unit: { target: "beat", unit: 0 }, slot: 0 },
      param: 0,
      value: 0.5,
    });
  });

  it("picks an effect from the searchable selector", async () => {
    render(<FxPanel />);
    fireEvent.click(screen.getByRole("combobox", { name: "Beat FX 1 slot 1 effect" }));
    const option = await screen.findByText("Noise");
    fireEvent.click(option);
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "set_effect",
      slot: { unit: { target: "beat", unit: 0 }, slot: 0 },
      effect: { provider: "builtin", id: "noise" },
    });
  });

  it("lists factory and user presets and saves a new one", async () => {
    mock.fxGetPresets?.mockImplementationOnce(() =>
      Promise.resolve({
        status: "ok" as const,
        data: {
          factory: [
            {
              effect: { provider: "builtin", id: "echo" },
              name: "Dub Echo",
              params: [],
              wet: 0.5,
              beat_division: 0.75,
            },
          ],
          user: [],
          chains: [{ name: "Big Room", slots: [] }],
        },
      }),
    );
    await hydrateFx();
    render(<FxPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Beat FX 1 presets" }));
    fireEvent.click(await screen.findByRole("button", { name: "Dub Echo" }));
    expect(applied().some((o) => o.kind === "apply_preset")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Big Room" }));
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "apply_chain",
      unit: { target: "beat", unit: 0 },
      name: "Big Room",
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Preset name" }), {
      target: { value: "Mine" },
    });
    fireEvent.click(within(screen.getByRole("region", { name: "Effect presets" })).getByRole("button", { name: "Save" }));
    expect(commands.fxSavePreset).toHaveBeenCalledWith(
      { unit: { target: "beat", unit: 0 }, slot: 0 },
      "Mine",
    );
  });

  it("edits the send buses", () => {
    render(<FxPanel />);
    const send = within(screen.getByRole("region", { name: "Send 1" }));
    expect(send.getByRole("combobox", { name: "Send 1 slot 1 effect" })).toBeTruthy();
    fireEvent.click(send.getByRole("button", { name: "Send 1 pre-fader" }));
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "set_send_tap", bus: 0, post_fader: false });
  });

  it("collapses", () => {
    render(<FxPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Collapse effects" }));
    expect(screen.queryByRole("region", { name: "Beat FX 1" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Expand effects" }));
    expect(screen.getByRole("region", { name: "Beat FX 1" })).toBeTruthy();
  });

  it("opens the pad FX editor from the panel", async () => {
    render(<FxPanel />);
    fireEvent.click(screen.getByRole("button", { name: "Pad FX" }));
    expect(await screen.findByRole("button", { name: "Pad 1, Echo" })).toBeTruthy();
  });
});

describe("channel strip FX", () => {
  it("has a bipolar Sound Color FX knob and send knobs per channel", () => {
    render(<ChannelStrip deck={1} />);
    useMixerStore.getState().apply(neutralMixer() as never);
    setKnob("Sound Color FX B", -0.5);
    expect(commands.fxApply).toHaveBeenCalledWith({ kind: "set_cfx", deck: 1, amount: -0.5 });
    expect(screen.getByText("L 50%")).toBeTruthy();
    setKnob("Send 2 B", 0.4);
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "set_send_level",
      deck: 1,
      bus: 1,
      level: 0.4,
    });
    expect(screen.getByRole("combobox", { name: "Sound Color FX type B" })).toBeTruthy();
  });

  it("keeps the Sound Color FX knob live beside a three-effect Multi unit", () => {
    const state = neutralFxState();
    const unit = state.units[0];
    const lead = unit?.slots[0];
    if (unit && lead) {
      unit.mode = "multi";
      unit.slots = unit.slots.map(() => ({ ...lead }));
    }
    useFxStore.getState().setState(state as never);
    render(<ChannelStrip deck={0} />);
    expect((slider("Sound Color FX A") as HTMLInputElement).disabled).toBe(false);
  });

  it("renders without FX state", () => {
    useFxStore.getState().reset();
    render(<ChannelStrip deck={0} />);
    expect(screen.queryByRole("slider", { name: "Sound Color FX A" })).toBeNull();
  });
});

describe("PadFxEditor", () => {
  it("lays out 32 pads in four groups", () => {
    render(<PadFxEditor />);
    for (const title of ["Pad FX 1", "Pad FX 1 shift", "Pad FX 2", "Pad FX 2 shift"]) {
      expect(screen.getByRole("region", { name: title })).toBeTruthy();
    }
    expect(screen.getAllByRole("button", { name: /^Pad \d+, / })).toHaveLength(32);
  });

  it("assigns the division, the release flag and the effect of a pad", () => {
    render(<PadFxEditor />);
    fireEvent.click(screen.getByRole("button", { name: "Pad 3, Echo" }));
    const settings = within(screen.getByRole("region", { name: "Pad 3 settings" }));
    fireEvent.click(settings.getByRole("radio", { name: "Pad 3 division 1/4" }));
    expect(commands.fxApply).toHaveBeenCalledWith({
      kind: "set_pad",
      index: 2,
      pad: { effect: { provider: "builtin", id: "echo" }, division: 0.25, release: false },
    });
    fireEvent.click(settings.getByRole("button", { name: "Pad 3 release FX" }));
    expect(commands.fxApply).toHaveBeenLastCalledWith({
      kind: "set_pad",
      index: 2,
      pad: { effect: { provider: "builtin", id: "echo" }, division: 0.5, release: true },
    });
  });

  it("switches a pad to an effect without beat sync and hides the division", async () => {
    render(<PadFxEditor />);
    fireEvent.click(screen.getByRole("button", { name: "Pad 2, Noise" }));
    const settings = within(screen.getByRole("region", { name: "Pad 2 settings" }));
    expect(settings.queryByRole("group", { name: "Pad 2 division" })).toBeNull();
    fireEvent.click(settings.getByRole("combobox", { name: "Pad 2 effect" }));
    fireEvent.click(await screen.findByText("Vinyl Brake"));
    const call = mock.fxApply?.mock.calls.at(-1)?.[0] as {
      kind: string;
      index: number;
      pad: { effect: { id: string }; division: number };
    };
    expect(call.kind).toBe("set_pad");
    expect(call.index).toBe(1);
    expect(call.pad.effect.id).toBe("vinyl_brake");
    // 1/2 is in the Vinyl Brake list, so it stays.
    expect(call.pad.division).toBe(0.5);
  });

  it("knows every effect it offers", () => {
    expect(fxEffects().some((e) => e.release)).toBe(true);
  });
});
