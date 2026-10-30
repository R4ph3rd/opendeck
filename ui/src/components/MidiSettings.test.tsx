import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Toaster, toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiMappingRowDto } from "../ipc/bindings";
import { midiStatus, type createBindingsMock } from "../test/bindingsMock";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const dialog = (await import("@tauri-apps/plugin-dialog")) as unknown as {
  open: ReturnType<typeof vi.fn>;
  save: ReturnType<typeof vi.fn>;
};
const { useAppStore } = await import("../state/app");
const { useMidiStore } = await import("../state/midi");
const { startEventBridge } = await import("../state/events");
const { MidiSettings } = await import("./MidiSettings");
const { TopBar } = await import("./TopBar");

const { commands } = bindings;
const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const row = (id: string, patch: Partial<MidiMappingRowDto> = {}): MidiMappingRowDto => ({
  id,
  label: null,
  source_ref: null,
  binding: { type: "note", channel: 1, number: 11 },
  action: { action: "play_pause", deck: 0 },
  layer: "base",
  value_kind: "button",
  verify: null,
  overridden: false,
  ...patch,
});

const ROWS: MidiMappingRowDto[] = [
  row("deck1.play"),
  row("deck1.cue", { binding: { type: "note", channel: 1, number: 12 }, action: { action: "cue", deck: 0 } }),
  row("deck1.eq.high", {
    binding: { type: "cc14", channel: 1, msb: 7, lsb: 39 },
    action: { action: "eq", deck: 0, band: "high" },
    value_kind: "absolute",
  }),
  row("fx.level", {
    binding: { type: "cc", channel: 5, number: 2 },
    action: { action: "fx_level", unit: { kind: "deck", index: 0 } },
    value_kind: "absolute",
    verify: "Q07 channel column",
  }),
  row("deck1.pad.hot_cue.1.shift", {
    binding: { type: "note", channel: 9, number: 0 },
    action: { action: "pad", deck: 0, mode: "hot_cue", index: 0, shifted: true },
    layer: "shift_deck1",
  }),
];

const tableRows = () => within(screen.getByRole("table", { name: "MIDI mapping" })).queryAllByRole("row").slice(1);

beforeEach(() => {
  vi.clearAllMocks();
  useMidiStore.getState().reset();
  useAppStore.getState().reset();
  commands.midiGetMapping.mockImplementation((() => ok(ROWS)) as never);
  commands.midiGetStatus.mockImplementation((() => ok(midiStatus())) as never);
  commands.midiListPorts.mockImplementation((() => ok([])) as never);
});

afterEach(() => {
  toast.dismiss();
  cleanup();
});

const mount = () =>
  render(
    <Toaster>
      <MidiSettings />
    </Toaster>,
  );

describe("MIDI settings: controller", () => {
  it("shows the connection and connects to a chosen device", async () => {
    commands.midiListPorts.mockImplementation((() =>
      ok([
        { name: "DDJ-FLX4 MIDI 1", has_input: true, has_output: true, matches_mapping: true },
        { name: "Other", has_input: true, has_output: false, matches_mapping: false },
      ])) as never);
    mount();
    expect(await screen.findByText("Looking for Pioneer DDJ-FLX4")).toBeTruthy();
    expect(screen.getByText(/165 values in this mapping are not confirmed/)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));
    await waitFor(() => expect(commands.midiConnect).toHaveBeenCalledWith(null));
  });

  it("shows a connected controller and disconnects it", async () => {
    commands.midiGetStatus.mockImplementation((() =>
      ok({ ...midiStatus(), connected: true, input: "DDJ-FLX4 MIDI 1", output: "DDJ-FLX4 MIDI 1" })) as never);
    mount();
    expect(await screen.findByText("Connected to DDJ-FLX4 MIDI 1")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => expect(commands.midiDisconnect).toHaveBeenCalled());
  });

  it("toggles connect at launch and fader start", async () => {
    mount();
    await screen.findByText(/Looking for/);
    fireEvent.click(screen.getByRole("switch", { name: "Connect at launch" }));
    await waitFor(() => expect(commands.midiSetAutoConnect).toHaveBeenCalledWith(false));
    fireEvent.click(screen.getByRole("switch", { name: "Fader start" }));
    await waitFor(() => expect(commands.midiSetFaderStart).toHaveBeenCalledWith(true));
  });

  it("shows the last MIDI error", async () => {
    commands.midiGetStatus.mockImplementation((() =>
      ok({ ...midiStatus(), last_error: "could not open MIDI port" })) as never);
    mount();
    expect(await screen.findByText("could not open MIDI port")).toBeTruthy();
  });
});

describe("MIDI settings: mapping table", () => {
  it("lists the rows with message, action, layer and the verify badge", async () => {
    mount();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));
    const fx = within(screen.getByTestId("mapping-row-fx.level"));
    expect(fx.getByText("CC 0x02 ch5")).toBeTruthy();
    expect(fx.getByText("FX level/depth")).toBeTruthy();
    expect(fx.getByText("verify").getAttribute("title")).toBe("Q07 channel column");
    const pad = within(screen.getByTestId("mapping-row-deck1.pad.hot_cue.1.shift"));
    expect(pad.getByText("shift_deck1")).toBeTruthy();
    expect(within(screen.getByTestId("mapping-row-deck1.play")).queryByText("verify")).toBeNull();
  });

  it("searches by control, message and action", async () => {
    mount();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));
    const search = screen.getByRole("textbox", { name: "Search the mapping" });
    fireEvent.change(search, { target: { value: "eq" } });
    expect(tableRows()).toHaveLength(1);
    fireEvent.change(search, { target: { value: "0x0c" } });
    expect(tableRows().map((r) => r.getAttribute("data-testid"))).toEqual(["mapping-row-deck1.cue"]);
    fireEvent.change(search, { target: { value: "verify" } });
    expect(tableRows()).toHaveLength(1);
    fireEvent.change(search, { target: { value: "zzz" } });
    expect(screen.getByText("Nothing matches")).toBeTruthy();
  });

  it("learn arms the backend, then applies the result and refreshes the table", async () => {
    mount();
    const stop = await startEventBridge();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));

    fireEvent.click(screen.getByRole("button", { name: "Learn deck1.play" }));
    await waitFor(() => expect(commands.midiLearnStart).toHaveBeenCalledWith("deck1.play"));
    expect(await screen.findByText("Learning")).toBeTruthy();
    expect(useMidiStore.getState().learning).toBe("deck1.play");

    const learned = ROWS.map((r) =>
      r.id === "deck1.play"
        ? { ...r, binding: { type: "note" as const, channel: 1, number: 20 }, overridden: true }
        : r,
    );
    commands.midiGetMapping.mockImplementation((() => ok(learned)) as never);
    act(() =>
      bindings.emit("midi-learned", {
        control_id: "deck1.play",
        binding: { type: "note", channel: 1, number: 20 },
        description: "Note On ch1 0x14 vel 127",
        error: null,
      }),
    );
    await waitFor(() =>
      expect(within(screen.getByTestId("mapping-row-deck1.play")).getByText("Note 0x14 ch1")).toBeTruthy(),
    );
    expect(within(screen.getByTestId("mapping-row-deck1.play")).getByText("custom")).toBeTruthy();
    expect(useMidiStore.getState().learning).toBeNull();
    expect(await screen.findByText(/deck1.play bound to Note On ch1 0x14/)).toBeTruthy();
    stop();
  });

  it("shows why a learned input was refused and leaves the table alone", async () => {
    mount();
    const stop = await startEventBridge();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));
    fireEvent.click(screen.getByRole("button", { name: "Learn deck1.play" }));
    await screen.findByText("Learning");
    act(() =>
      bindings.emit("midi-learned", {
        control_id: "deck1.play",
        binding: null,
        description: "Note On ch1 0x0C vel 127",
        error: "input already used by deck1.cue",
      }),
    );
    expect(await screen.findByText(/input already used by deck1.cue/)).toBeTruthy();
    expect(useMidiStore.getState().learning).toBeNull();
    expect(screen.queryByText("Learning")).toBeNull();
    stop();
  });

  it("cancels learning", async () => {
    mount();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));
    fireEvent.click(screen.getByRole("button", { name: "Learn deck1.play" }));
    await screen.findByText("Learning");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(commands.midiLearnCancel).toHaveBeenCalled());
    expect(screen.queryByText("Learning")).toBeNull();
  });

  it("resets one binding and all bindings", async () => {
    const custom = ROWS.map((r) => (r.id === "deck1.cue" ? { ...r, overridden: true } : r));
    commands.midiGetMapping.mockImplementation((() => ok(custom)) as never);
    commands.midiClearOverride.mockImplementation((() => ok(custom)) as never);
    mount();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));
    expect((screen.getByRole("button", { name: "Reset deck1.play" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Reset deck1.cue" }));
    await waitFor(() => expect(commands.midiClearOverride).toHaveBeenCalledWith("deck1.cue"));
    fireEvent.click(screen.getByRole("button", { name: "Reset all" }));
    await waitFor(() => expect(commands.midiResetOverrides).toHaveBeenCalled());
  });

  it("exports and imports through the file dialogs", async () => {
    dialog.save.mockResolvedValue("/tmp/mapping.json");
    dialog.open.mockResolvedValue("/tmp/other.json");
    mount();
    await waitFor(() => expect(tableRows()).toHaveLength(ROWS.length));
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() => expect(commands.midiExportMapping).toHaveBeenCalledWith("/tmp/mapping.json"));
    fireEvent.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => expect(commands.midiImportMapping).toHaveBeenCalledWith("/tmp/other.json"));

    // A cancelled dialog does nothing.
    dialog.save.mockResolvedValue(null);
    commands.midiExportMapping.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await new Promise((r) => setTimeout(r, 20));
    expect(commands.midiExportMapping).not.toHaveBeenCalled();
  });
});

describe("TopBar controller indicator", () => {
  it("shows no controller, then the connected device, and opens the MIDI page", async () => {
    render(<TopBar />);
    const stop = await startEventBridge();
    expect(await screen.findByRole("button", { name: /MIDI controller: No controller/ })).toBeTruthy();

    act(() =>
      bindings.emit("midi-connection-changed", {
        ...midiStatus(),
        connected: true,
        input: "DDJ-FLX4 MIDI 1",
      }),
    );
    const badge = await screen.findByRole("button", { name: /MIDI controller: DDJ-FLX4 MIDI 1/ });
    fireEvent.click(badge);
    expect(useAppStore.getState().settingsOpen).toBe(true);
    expect(useAppStore.getState().settingsPage).toBe("midi");

    fireEvent.click(screen.getByRole("button", { name: "MIDI monitor" }));
    expect(useAppStore.getState().midiMonitorOpen).toBe(true);
    stop();
  });
});
