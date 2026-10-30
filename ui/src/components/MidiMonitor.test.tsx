import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiMonitorEntryDto } from "../ipc/bindings";
import type { createBindingsMock } from "../test/bindingsMock";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("../state/app");
const { MONITOR_LIMIT, useMidiStore } = await import("../state/midi");
const { startEventBridge } = await import("../state/events");
const { MidiMonitor, MidiMonitorDialog } = await import("./MidiMonitor");

const { commands } = bindings;

const entry = (patch: Partial<MidiMonitorEntryDto> = {}): MidiMonitorEntryDto => ({
  ts_us: 1_000_000,
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

const unmapped = (ts: number): MidiMonitorEntryDto =>
  entry({ ts_us: ts, bytes: [0xb0, 0x40, 0x01], text: "CC ch1 0x40 value 1", label: { kind: "unmapped" } });

const out = (ts: number): MidiMonitorEntryDto =>
  entry({
    ts_us: ts,
    direction: "out",
    bytes: [0x90, 0x0b, 0x7f],
    text: "Note On ch1 0x0B vel 127",
    label: { kind: "output" },
  });

const bodyRows = () => screen.queryAllByRole("row").slice(1);

beforeEach(() => {
  vi.clearAllMocks();
  useMidiStore.getState().reset();
  useAppStore.getState().reset();
});

afterEach(cleanup);

describe("MidiMonitor", () => {
  it("shows in and out lines with the decoded control and time from the first line", () => {
    render(<MidiMonitor />);
    act(() => {
      useMidiStore.getState().pushMonitor([entry(), out(1_250_000)]);
    });
    const rows = bodyRows();
    expect(rows).toHaveLength(2);
    const first = within(rows[0] as HTMLElement);
    expect(first.getByText("IN")).toBeTruthy();
    expect(first.getByText("90 0B 7F")).toBeTruthy();
    expect(first.getByText("deck1.play: Deck 1 Play/Pause")).toBeTruthy();
    expect(first.getByText("0.000 s")).toBeTruthy();
    const second = within(rows[1] as HTMLElement);
    expect(second.getByText("OUT")).toBeTruthy();
    expect(second.getByText("0.250 s")).toBeTruthy();
  });

  it("marks an unconfirmed value", () => {
    render(<MidiMonitor />);
    act(() => {
      useMidiStore.getState().pushMonitor([
        entry({
          label: { kind: "control", id: "fx.level", label: null, action: null, verify: "Q07 channel" },
        }),
      ]);
    });
    expect(screen.getByText("verify").getAttribute("title")).toBe("Q07 channel");
  });

  it("filters by direction and text", () => {
    render(<MidiMonitor />);
    act(() => {
      useMidiStore.getState().pushMonitor([entry(), unmapped(2_000_000), out(3_000_000)]);
    });
    expect(bodyRows()).toHaveLength(3);

    fireEvent.click(screen.getByRole("button", { name: "Show messages out" }));
    expect(bodyRows()).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Show messages out" }));

    fireEvent.change(screen.getByRole("textbox", { name: "Filter messages" }), {
      target: { value: "not in the mapping" },
    });
    expect(bodyRows()).toHaveLength(1);
    expect(screen.getByTestId("monitor-count").textContent).toContain("1 of 3");
  });

  it("pause stops collecting, resume continues, clear empties the log", () => {
    render(<MidiMonitor />);
    act(() => useMidiStore.getState().pushMonitor([entry()]));
    fireEvent.click(screen.getByRole("button", { name: /Pause/ }));
    act(() => useMidiStore.getState().pushMonitor([unmapped(2_000_000)]));
    expect(bodyRows()).toHaveLength(1);
    expect(screen.getByTestId("monitor-count").textContent).toContain("paused");

    fireEvent.click(screen.getByRole("button", { name: /Resume/ }));
    act(() => useMidiStore.getState().pushMonitor([unmapped(3_000_000)]));
    expect(bodyRows()).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(screen.getByText("No messages")).toBeTruthy();
    expect(useMidiStore.getState().monitorOrigin).toBeNull();
  });

  it("keeps the log bounded", () => {
    const many = Array.from({ length: MONITOR_LIMIT + 50 }, (_, i) => unmapped(i));
    useMidiStore.getState().pushMonitor(many);
    expect(useMidiStore.getState().monitor).toHaveLength(MONITOR_LIMIT);
    expect(useMidiStore.getState().monitor[0]?.ts_us).toBe(50);
  });

  it("receives batches from the backend event", async () => {
    render(<MidiMonitor />);
    const stop = await startEventBridge();
    act(() => bindings.emit("midi-monitor", [entry(), unmapped(2_000_000)]));
    await waitFor(() => expect(bodyRows()).toHaveLength(2));
    stop();
  });
});

describe("MidiMonitorDialog", () => {
  it("starts the backend stream while open and stops it when closed", async () => {
    render(<MidiMonitorDialog />);
    expect(commands.midiSetMonitor).not.toHaveBeenCalled();
    act(() => useAppStore.getState().setMidiMonitorOpen(true));
    await waitFor(() => expect(commands.midiSetMonitor).toHaveBeenCalledWith(true));
    expect(await screen.findByRole("dialog", { name: "MIDI monitor" })).toBeTruthy();
    act(() => useAppStore.getState().setMidiMonitorOpen(false));
    await waitFor(() => expect(commands.midiSetMonitor).toHaveBeenLastCalledWith(false));
  });
});
