import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Toaster } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiMonitorEntryDto } from "../ipc/bindings";
import { midiStatus, type createBindingsMock } from "../test/bindingsMock";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const dialog = (await import("@tauri-apps/plugin-dialog")) as unknown as {
  open: ReturnType<typeof vi.fn>;
};
const { useMidiStore } = await import("../state/midi");
const { usePrefsStore } = await import("../state/prefs");
const { FirstRunDialog } = await import("./FirstRun");

const { commands } = bindings;
const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const flx4Device = {
  name: "DDJ-FLX4",
  is_default: false,
  max_channels: 4,
  default_sample_rate: 48_000,
  sample_rate_ranges: [[44_100, 48_000]],
  buffer_frames: [64, 2048],
};

const playPress: MidiMonitorEntryDto = {
  ts_us: 1,
  direction: "in",
  bytes: [0x90, 0x0b, 0x7f],
  text: "Note On ch1 0x0B vel 127",
  label: {
    kind: "control",
    id: "deck1.play",
    label: "PLAY/PAUSE Deck 1",
    action: { action: "play_pause", deck: 0 },
    verify: null,
  },
};

async function openAt(step: "welcome" | "audio" | "controller" | "library" | "tour") {
  render(
    <Toaster>
      <FirstRunDialog />
    </Toaster>,
  );
  act(() => usePrefsStore.getState().openFirstRun());
  const order = ["welcome", "audio", "controller", "library", "tour"];
  for (let i = 0; i < order.indexOf(step); i++) {
    fireEvent.click(await screen.findByRole("button", { name: "Next" }));
  }
}

beforeEach(() => {
  usePrefsStore.setState({ firstRunOpen: false, firstRunDone: false });
  useMidiStore.getState().reset();
  vi.clearAllMocks();
  bindings.prefs.clear();
});

afterEach(() => cleanup());

describe("first run", () => {
  it("starts on the welcome step and walks through all five", async () => {
    await openAt("welcome");
    expect(screen.getByText(/Step 1 of 5/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText(/Step 2 of 5/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByText(/Step 1 of 5/)).toBeTruthy();
  });

  it("preselects the FLX4 with master 1/2 and headphones 3/4", async () => {
    commands.listAudioDevices.mockImplementationOnce(() =>
      ok([
        { ...flx4Device, is_default: false },
        { ...flx4Device, name: "Speakers", is_default: true },
      ]),
    );
    await openAt("audio");
    expect(await screen.findByText("DDJ-FLX4 found")).toBeTruthy();
    expect(screen.getByText(/Master is on outputs 1\/2 and headphones on 3\/4/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Use this output" }));
    await waitFor(() => expect(commands.setAudioSettings).toHaveBeenCalled());
    expect(commands.setAudioSettings).toHaveBeenCalledWith(
      expect.objectContaining({
        device: "DDJ-FLX4",
        master: { left: 0, right: 1 },
        headphones: { left: 2, right: 3 },
      }),
    );
  });

  it("plays a test tone on each output", async () => {
    commands.listAudioDevices.mockImplementationOnce(() => ok([flx4Device]));
    await openAt("audio");
    fireEvent.click(await screen.findByRole("button", { name: /test tone on master 1\/2/ }));
    await waitFor(() =>
      expect(commands.playTestTone).toHaveBeenCalledWith(null, "DDJ-FLX4", {
        left: 0,
        right: 1,
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: /test tone on headphones 3\/4/ }));
    await waitFor(() =>
      expect(commands.playTestTone).toHaveBeenCalledWith(null, "DDJ-FLX4", {
        left: 2,
        right: 3,
      }),
    );
  });

  it("shows a toast when the tone cannot play", async () => {
    commands.playTestTone.mockImplementationOnce((() =>
      Promise.resolve({
        status: "error" as const,
        error: { kind: "audio", message: "device busy" },
      })) as never);
    await openAt("audio");
    fireEvent.click(await screen.findByRole("button", { name: /test tone on master/ }));
    expect(await screen.findByText("Audio problem: device busy")).toBeTruthy();
  });

  it("detects the controller and confirms a PLAY press from the monitor stream", async () => {
    commands.midiListPorts.mockImplementation((() =>
      ok([
        {
          name: "DDJ-FLX4",
          has_input: true,
          has_output: true,
          matches_mapping: true,
        },
      ])) as never);
    commands.midiConnect.mockImplementation(() =>
      ok({ ...midiStatus(), connected: true, input: "DDJ-FLX4" }),
    );
    await openAt("controller");
    expect(await screen.findByText("Found: DDJ-FLX4")).toBeTruthy();
    await waitFor(() => expect(commands.midiSetMonitor).toHaveBeenCalledWith(true));
    expect(screen.getByTestId("play-check").textContent).toContain("Press PLAY on deck 1");
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(commands.midiConnect).toHaveBeenCalledWith("DDJ-FLX4"));
    act(() => useMidiStore.getState().pushMonitor([playPress]));
    expect(screen.getByTestId("play-check").textContent).toContain("PLAY on deck 1 received");
  });

  it("stops the monitor when leaving the controller step", async () => {
    await openAt("controller");
    await waitFor(() => expect(commands.midiSetMonitor).toHaveBeenCalledWith(true));
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    await waitFor(() => expect(commands.midiSetMonitor).toHaveBeenCalledWith(false));
  });

  it("imports a chosen music folder and remembers it", async () => {
    dialog.open.mockResolvedValueOnce("/music/techno");
    await openAt("library");
    fireEvent.click(screen.getByRole("button", { name: /Add music folder/ }));
    await waitFor(() =>
      expect(commands.importFolder).toHaveBeenCalledWith(
        "/music/techno",
        expect.objectContaining({
          features: expect.objectContaining({ tempo: true }),
        }),
      ),
    );
    expect(await screen.findByText("/music/techno")).toBeTruthy();
    expect(usePrefsStore.getState().library.folders).toContain("/music/techno");
  });

  it("offers the rekordbox import", async () => {
    dialog.open.mockResolvedValueOnce("/exports/collection.xml");
    await openAt("library");
    fireEvent.click(screen.getByRole("button", { name: "Import rekordbox XML" }));
    await waitFor(() =>
      expect(commands.importRekordboxXml).toHaveBeenCalledWith("/exports/collection.xml", null),
    );
  });

  it("finishes with the panel tour and stores completion", async () => {
    await openAt("tour");
    expect(screen.getByRole("list", { name: "Panels" }).children.length).toBeGreaterThanOrEqual(5);
    fireEvent.click(screen.getByRole("button", { name: "Finish" }));
    await waitFor(() => expect(usePrefsStore.getState().firstRunDone).toBe(true));
    expect(commands.setUiPref).toHaveBeenCalledWith("firstRunDone", "true");
    expect(screen.queryByText("Set up OpenDeck")).toBeNull();
  });

  it("counts skipping as done so it is shown once", async () => {
    await openAt("welcome");
    fireEvent.click(screen.getByRole("button", { name: "Skip setup" }));
    await waitFor(() => expect(commands.setUiPref).toHaveBeenCalledWith("firstRunDone", "true"));
  });

  it("opens on first launch only, and again on request", async () => {
    await usePrefsStore.getState().hydrate();
    expect(usePrefsStore.getState().firstRunOpen).toBe(true);
    usePrefsStore.getState().completeFirstRun();
    await usePrefsStore.getState().hydrate();
    expect(usePrefsStore.getState().firstRunOpen).toBe(false);
    usePrefsStore.getState().openFirstRun();
    expect(usePrefsStore.getState().firstRunOpen).toBe(true);
  });

  it("does not open without a backend", async () => {
    commands.getUiPref.mockImplementationOnce(() => Promise.reject(new Error("no ipc")));
    await usePrefsStore.getState().hydrate();
    expect(usePrefsStore.getState().firstRunOpen).toBe(false);
  });
});
