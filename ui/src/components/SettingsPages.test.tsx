import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Toaster, toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SettingsPage } from "../state/app";
import type { createBindingsMock } from "../test/bindingsMock";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const dialog = (await import("@tauri-apps/plugin-dialog")) as unknown as {
  open: ReturnType<typeof vi.fn>;
};
const { useAppStore } = await import("../state/app");
const { useLibraryStore } = await import("../state/library");
const { usePrefsStore, applyTheme, DEFAULT_LIBRARY_OPTIONS, LAYOUT_KEY } =
  await import("../state/prefs");
const { useShortcutStore } = await import("../state/shortcuts");
const { SHORTCUTS, findShortcut, setActiveShortcuts } = await import("../lib/shortcuts");
const { SettingsDialog } = await import("./SettingsDialog");

const { commands } = bindings;
const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

function renderSettings(page: SettingsPage) {
  render(
    <Toaster>
      <SettingsDialog />
    </Toaster>,
  );
  act(() => useAppStore.getState().openSettings(page));
}

beforeEach(() => {
  useAppStore.getState().reset();
  useLibraryStore.getState().reset();
  usePrefsStore.setState({
    theme: "dark",
    layoutPresets: [],
    layoutRevision: 0,
    library: DEFAULT_LIBRARY_OPTIONS,
    waveformZoom: 2,
  });
  useShortcutStore.getState().resetAll();
  bindings.prefs.clear();
  localStorage.clear();
  vi.clearAllMocks();
});

afterEach(() => {
  toast.dismiss();
  cleanup();
  setActiveShortcuts(SHORTCUTS);
  document.documentElement.removeAttribute("data-theme");
});

describe("settings navigation", () => {
  it("lists every page and keeps Audio and MIDI", async () => {
    renderSettings("audio");
    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    for (const label of ["Audio", "MIDI", "Library", "Interface", "Keyboard", "Advanced"]) {
      expect(within(nav).getByRole("button", { name: label })).toBeTruthy();
    }
    fireEvent.click(within(nav).getByRole("button", { name: "Keyboard" }));
    expect(await screen.findByRole("list", { name: "Shortcuts" })).toBeTruthy();
  });
});

describe("Library page", () => {
  it("adds a folder, imports it and remembers it", async () => {
    dialog.open.mockResolvedValueOnce("/music");
    renderSettings("library");
    fireEvent.click(screen.getByRole("button", { name: /Add folder/ }));
    await waitFor(() => expect(commands.importFolder).toHaveBeenCalled());
    expect(await screen.findByText("/music")).toBeTruthy();
    await waitFor(() =>
      expect(commands.setUiPref).toHaveBeenCalledWith(
        "libraryOptions",
        expect.stringContaining("/music"),
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "Rescan" }));
    await waitFor(() => expect(commands.importFolder).toHaveBeenCalledTimes(2));
  });

  it("turns auto-analyse off so imports skip analysis", async () => {
    renderSettings("library");
    fireEvent.click(screen.getByRole("switch", { name: "Analyse on import" }));
    expect(usePrefsStore.getState().library.autoAnalyse).toBe(false);
    dialog.open.mockResolvedValueOnce("/music");
    fireEvent.click(screen.getByRole("button", { name: /Add folder/ }));
    await waitFor(() =>
      expect(commands.importFolder).toHaveBeenCalledWith("/music", {
        features: { tempo: false, key: false, loudness: false, peaks: false },
        target_playlist: null,
      }),
    );
  });

  it("switches key notation", () => {
    renderSettings("library");
    fireEvent.click(screen.getByRole("radio", { name: "Musical" }));
    expect(useLibraryStore.getState().keyNotation).toBe("musical");
  });

  it("re-analyses every track", async () => {
    commands.queryTracks.mockImplementationOnce((() =>
      ok({ rows: [{ id: 4 }, { id: 9 }], total: 2 })) as never);
    renderSettings("library");
    fireEvent.click(screen.getByRole("button", { name: "Re-analyse all" }));
    await waitFor(() =>
      expect(commands.reanalyzeTracks).toHaveBeenCalledWith([4, 9], {
        tempo: true,
        key: true,
        loudness: true,
        peaks: true,
      }),
    );
  });
});

describe("Interface page", () => {
  it("applies and stores the theme", async () => {
    renderSettings("interface");
    fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    expect(usePrefsStore.getState().theme).toBe("light");
    applyTheme(usePrefsStore.getState().theme);
    expect(document.documentElement.dataset.theme).toBe("light");
    await waitFor(() => expect(commands.setUiPref).toHaveBeenCalledWith("theme", '"light"'));
  });

  it("saves the current layout and restores it", async () => {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify({ stage: 60, pads: 20, library: 20 }));
    renderSettings("interface");
    fireEvent.change(screen.getByLabelText("Layout name"), {
      target: { value: "Wide stage" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save current layout" }));
    expect(usePrefsStore.getState().layoutPresets).toEqual([
      { name: "Wide stage", layout: { stage: 60, pads: 20, library: 20 } },
    ]);
    await waitFor(() =>
      expect(commands.setUiPref).toHaveBeenCalledWith("layoutPresets", expect.any(String)),
    );

    // Restore the built-in default, then the saved one.
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[0] as HTMLElement);
    expect(JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "{}")).toEqual({
      stage: 45,
      pads: 25,
      library: 30,
    });
    expect(usePrefsStore.getState().layoutRevision).toBe(1);
    fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[1] as HTMLElement);
    expect(JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "{}")).toEqual({
      stage: 60,
      pads: 20,
      library: 20,
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete Wide stage" }));
    expect(usePrefsStore.getState().layoutPresets).toEqual([]);
  });

  it("stores the default waveform zoom", () => {
    usePrefsStore.getState().setWaveformZoom(4);
    expect(usePrefsStore.getState().waveformZoom).toBe(4);
    usePrefsStore.getState().setWaveformZoom(99);
    expect(usePrefsStore.getState().waveformZoom).toBe(5);
  });
});

describe("Keyboard page", () => {
  const row = (id: string) => screen.getByTestId(`shortcut-${id}`);

  it("lists every action with its current key", () => {
    renderSettings("keyboard");
    const list = screen.getByRole("list", { name: "Shortcuts" });
    expect(list.children).toHaveLength(SHORTCUTS.length);
    expect(within(row("0:play")).getByText("Space")).toBeTruthy();
    expect(within(row("1:sync")).getByText("K")).toBeTruthy();
  });

  it("rebinds by key capture, applies it at once and persists it", async () => {
    renderSettings("keyboard");
    fireEvent.click(within(row("0:play")).getByRole("button", { name: /Rebind/ }));
    expect(screen.getByRole("status").textContent).toContain("Press a key for Deck 1 Play / pause");
    fireEvent.keyDown(window, { code: "KeyG" });
    expect(within(row("0:play")).getByText("G")).toBeTruthy();
    expect(findShortcut("KeyG")?.action.kind).toBe("play");
    expect(findShortcut("Space")).toBeUndefined();
    await waitFor(() =>
      expect(commands.setUiPref).toHaveBeenCalledWith("shortcuts", '{"0:play":"KeyG"}'),
    );
  });

  it("cancels a capture with Escape and refuses reserved keys", () => {
    renderSettings("keyboard");
    fireEvent.click(within(row("0:play")).getByRole("button", { name: /Rebind/ }));
    fireEvent.keyDown(window, { code: "Tab" });
    expect(screen.getByRole("status").textContent).toContain("Tab cannot be used");
    fireEvent.keyDown(window, { code: "ControlLeft", ctrlKey: true });
    expect(screen.getByRole("status").textContent).toContain("Ctrl");
    fireEvent.keyDown(window, { code: "Escape" });
    expect(within(row("0:play")).getByText("Space")).toBeTruthy();
    expect(Object.keys(useShortcutStore.getState().overrides)).toHaveLength(0);
  });

  it("flags a conflict on both actions", () => {
    renderSettings("keyboard");
    fireEvent.click(within(row("0:play")).getByRole("button", { name: /Rebind/ }));
    fireEvent.keyDown(window, { code: "KeyS" });
    expect(within(row("0:play")).getByText(/Conflict with Deck 1 Sync/)).toBeTruthy();
    expect(within(row("0:sync")).getByText(/Conflict with Deck 1 Play/)).toBeTruthy();
    expect(within(row("1:play")).queryByText(/Conflict/)).toBeNull();
    // Clearing one side resolves it.
    fireEvent.click(within(row("0:play")).getByRole("button", { name: /Clear/ }));
    expect(screen.queryByText(/Conflict/)).toBeNull();
  });

  it("resets one action and all actions to the defaults", () => {
    renderSettings("keyboard");
    fireEvent.click(within(row("0:play")).getByRole("button", { name: /Rebind/ }));
    fireEvent.keyDown(window, { code: "KeyG" });
    fireEvent.click(within(row("0:sync")).getByRole("button", { name: /Rebind/ }));
    fireEvent.keyDown(window, { code: "KeyH" });
    fireEvent.click(within(row("0:play")).getByRole("button", { name: /Reset/ }));
    expect(within(row("0:play")).getByText("Space")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reset all to defaults" }));
    expect(within(row("0:sync")).getByText("S")).toBeTruthy();
    expect(useShortcutStore.getState().overrides).toEqual({});
  });

  it("loads stored bindings at startup", async () => {
    bindings.prefs.set("shortcuts", '{"1:cue":"KeyV"}');
    await useShortcutStore.getState().hydrate();
    expect(findShortcut("KeyV")).toMatchObject({
      deck: 1,
      action: { kind: "cue" },
    });
    expect(findShortcut("F2")).toBeUndefined();
  });

  it("ignores corrupt stored bindings", async () => {
    bindings.prefs.set("shortcuts", "not json");
    await useShortcutStore.getState().hydrate();
    expect(findShortcut("Space")?.action.kind).toBe("play");
  });
});

describe("Advanced page", () => {
  it("shows the database location and opens the logs folder", async () => {
    renderSettings("advanced");
    expect((await screen.findByTestId("database-path")).textContent).toBe("/data/library.db");
    fireEvent.click(screen.getByRole("button", { name: "Open logs folder" }));
    await waitFor(() => expect(commands.openLogsFolder).toHaveBeenCalled());
  });

  it("copies diagnostics to the clipboard", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(globalThis.navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    renderSettings("advanced");
    fireEvent.click(await screen.findByRole("button", { name: "Copy diagnostics" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledOnce());
    expect(await screen.findByText("Diagnostics copied.")).toBeTruthy();
  });

  it("resets interface settings only after confirmation", async () => {
    usePrefsStore.getState().setTheme("light");
    renderSettings("advanced");
    fireEvent.click(screen.getByRole("button", { name: "Reset settings" }));
    expect(commands.resetUiPrefs).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm reset" }));
    await waitFor(() => expect(commands.resetUiPrefs).toHaveBeenCalled());
    await waitFor(() => expect(usePrefsStore.getState().theme).toBe("dark"));
  });

  it("reopens the first-run guide", () => {
    renderSettings("advanced");
    fireEvent.click(screen.getByRole("button", { name: "Show again" }));
    expect(usePrefsStore.getState().firstRunOpen).toBe(true);
  });
});

describe("Audio page", () => {
  it("shows unavailable hosts disabled with what they need", async () => {
    commands.listAudioHosts.mockImplementationOnce((() =>
      ok([
        { kind: "wasapi", name: "WASAPI", available: true, is_default: true },
        { kind: "asio", name: "ASIO", available: false, is_default: false },
      ])) as never);
    renderSettings("audio");
    fireEvent.click(await screen.findByRole("combobox", { name: "Host API" }));
    const asio = await screen.findByRole("option", { name: "ASIO: build with the asio feature" });
    expect(asio.getAttribute("aria-disabled")).toBe("true");
  });

  it("reports measured latency and dropouts from the status", async () => {
    useAppStore.getState().setAudio({
      running: true,
      backend: "device",
      host: "ALSA",
      device: "Test Device",
      sample_rate: 48_000,
      channels: 4,
      buffer_frames: 256,
      latency_ms: 10.66,
      latency_measured: true,
      master: { left: 0, right: 1 },
      headphones: { left: 2, right: 3 },
      stereo_fallback: false,
      xruns: 3,
      error: null,
    });
    renderSettings("audio");
    expect((await screen.findByTestId("latency")).textContent).toBe(
      "Output latency 10.7 ms (measured).",
    );
    expect(screen.getByTestId("xruns").textContent).toBe("3");
  });

  it("plays the test tone through the engine and applies changes first", async () => {
    renderSettings("audio");
    const master = await screen.findByRole("button", { name: "Test tone on master" });
    expect(
      (screen.getByRole("button", { name: "Test tone on booth" }) as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.click(master);
    await waitFor(() => expect(commands.playTestTone).toHaveBeenCalledWith("master"));
    expect(commands.setAudioSettings).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("switch", { name: "Headphones mono" }));
    fireEvent.click(screen.getByRole("button", { name: "Test tone on headphones" }));
    await waitFor(() => expect(commands.playTestTone).toHaveBeenCalledWith("headphones"));
    expect(commands.setAudioSettings).toHaveBeenCalledWith(
      expect.objectContaining({ headphones_mono: true }),
    );
  });

  it("sends talk-over and mic changes live and saves routing on apply", async () => {
    renderSettings("audio");
    fireEvent.click(await screen.findByRole("switch", { name: "Talk-over" }));
    await waitFor(() =>
      expect(commands.setTalkOver).toHaveBeenCalledWith({
        enabled: true,
        depth_db: 12,
        threshold_db: -40,
      }),
    );
    fireEvent.click(await screen.findByRole("switch", { name: "Mic on" }));
    await waitFor(() => expect(commands.setMicOn).toHaveBeenCalledWith(true));
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(commands.setAudioSettings).toHaveBeenCalledTimes(1));
    expect((commands.setAudioSettings.mock.calls as unknown[][])[0]?.[0]).toMatchObject({
      talkover: { enabled: true },
    });
  });
});

describe("Library BPM range", () => {
  it("sends the range to the analyzer", async () => {
    usePrefsStore.getState().setLibrary({ bpmMin: 80 });
    await waitFor(() =>
      expect(commands.setBpmRange).toHaveBeenCalledWith({ min_bpm: 80, max_bpm: 180 }),
    );
  });

  it("loads the analyzer's range at startup", async () => {
    commands.getBpmRange.mockImplementationOnce((() => ok({ min_bpm: 90, max_bpm: 170 })) as never);
    await usePrefsStore.getState().hydrate();
    expect(usePrefsStore.getState().library).toMatchObject({ bpmMin: 90, bpmMax: 170 });
  });
});
