import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "./test/bindingsMock";
import { runningAudio, telemetry, track } from "./test/fixtures";

vi.mock("./ipc/bindings", async () => (await import("./test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("./ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("./state/app");
const { telemetryStore } = await import("./state/telemetry");
const { releaseAllCues } = await import("./state/actions");
const { App } = await import("./App");
const { BAND_MIN_PX, DECK_COLUMN_MIN_PX, TOP_BAR_PX } = await import("./lib/layout");
const { DEFAULT_FX_LAYOUT, DEFAULT_LAYOUT } = await import("./state/prefs");

const { commands } = bindings;

beforeEach(() => {
  useAppStore.getState().reset();
  telemetryStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(() => {
  releaseAllCues();
  toast.dismiss();
  cleanup();
});

/** Renders the app and waits until the event bridge has subscribed. */
async function mountApp() {
  render(<App />);
  await waitFor(() => expect(bindings.events.engineTelemetry.listen).toHaveBeenCalled());
  await waitFor(() => expect(useAppStore.getState().audio).not.toBeNull());
}

describe("App", () => {
  it("lays out the top bar and the stacked bands: waveforms, decks with the mixer, pads, library", async () => {
    await mountApp();
    expect(screen.getByRole("heading", { name: "OpenDeck" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Waveforms" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Deck 1" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Deck 2" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Mixer" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Pads and effects" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Library" })).toBeTruthy();
    expect(screen.getAllByText("No track loaded")).toHaveLength(2);
    // One resizable band per section, in order.
    const ids = [...document.querySelectorAll("main > div > [data-panel]")].map((el) => el.id);
    expect(ids).toEqual(["waves", "decks", "pads", "library"]);
    // Separators only: three between the four bands.
    expect(document.querySelectorAll('main > div > [role="separator"]')).toHaveLength(3);
  });

  it("shows the effects band between the decks and the pads from the top bar", async () => {
    await mountApp();
    expect(screen.queryByRole("region", { name: "Effects" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show effects" }));
    expect(screen.getByRole("region", { name: "Effects" })).toBeTruthy();
    const ids = [...document.querySelectorAll("main > div > [data-panel]")].map((el) => el.id);
    expect(ids).toEqual(["waves", "decks", "fx", "pads", "library"]);
    fireEvent.click(screen.getByRole("button", { name: "Show effects" }));
    expect(screen.queryByRole("region", { name: "Effects" })).toBeNull();
  });

  it("keeps every band at or above its minimum size in a 1280 x 720 window", () => {
    const usable = 720 - TOP_BAR_PX;
    const base = BAND_MIN_PX.waves + BAND_MIN_PX.decks + BAND_MIN_PX.pads + BAND_MIN_PX.library;
    expect(base).toBeLessThanOrEqual(usable);
    expect(base + BAND_MIN_PX.fx).toBeLessThanOrEqual(usable);
    for (const layout of [DEFAULT_LAYOUT, DEFAULT_FX_LAYOUT]) {
      const total = Object.values(layout).reduce((sum, v) => sum + v, 0);
      expect(total).toBeCloseTo(100, 6);
      for (const [band, percent] of Object.entries(layout)) {
        const min = BAND_MIN_PX[band as keyof typeof BAND_MIN_PX];
        expect((percent / 100) * usable).toBeGreaterThanOrEqual(min);
      }
    }
    // The deck row fits two decks and the mixer at 1280 px.
    expect(2 * DECK_COLUMN_MIN_PX.deck + DECK_COLUMN_MIN_PX.mixer).toBeLessThanOrEqual(1280);
  });

  it("shows the output device in the top bar", async () => {
    await mountApp();
    expect(screen.getByRole("button", { name: /Audio output: Test Device 48 kHz/ })).toBeTruthy();
  });

  it("routes track-loaded and telemetry events into the deck", async () => {
    await mountApp();
    act(() => {
      bindings.emit("track-loaded", { deck: 0, track, frames: 200 * 48_000 });
      bindings.emit("engine-telemetry", telemetry(1, { loaded: true, duration: 200 * 48_000 }));
    });
    expect(screen.getByTestId("title-0").textContent).toBe("Night Drive");
    const deckOne = within(screen.getByRole("region", { name: "Deck 1" }));
    expect(deckOne.getByRole("button", { name: "PLAY" })).toHaveProperty("disabled", false);
    const deckTwo = within(screen.getByRole("region", { name: "Deck 2" }));
    expect(deckTwo.getByRole("button", { name: "PLAY" })).toHaveProperty("disabled", true);
  });

  it("toasts a decode failure and keeps the deck usable", async () => {
    await mountApp();
    act(() =>
      bindings.emit("track-load-failed", {
        deck: 0,
        path: "/music/bad.mp3",
        message: "corrupt audio",
      }),
    );
    expect(await screen.findByText(/Could not load bad.mp3: corrupt audio/)).toBeTruthy();
    expect(useAppStore.getState().decks[0]?.loading).toBe(false);
  });

  it("toasts a lost audio device and marks audio off", async () => {
    await mountApp();
    act(() => {
      bindings.emit("engine-event", { type: "device_lost" });
      bindings.emit("audio-device-changed", {
        ...runningAudio,
        running: false,
        error: "Output device lost",
      });
    });
    expect((await screen.findAllByText(/Audio device lost/)).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: /Audio output: Audio off/ })).toBeTruthy();
  });

  it("ignores engine events that need no message", async () => {
    await mountApp();
    act(() => bindings.emit("engine-event", { type: "track_ended", deck: 0 }));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.queryByText(/audio/i, { selector: "[data-variant]" })).toBeNull();
  });

  it("Space toggles deck A and F1 holds CUE", async () => {
    await mountApp();
    fireEvent.keyDown(window, { code: "Space", key: " " });
    expect(commands.togglePlaying).toHaveBeenCalledWith(0);
    fireEvent.keyDown(window, { code: "Space", key: " ", repeat: true });
    expect(commands.togglePlaying).toHaveBeenCalledTimes(1);

    fireEvent.keyDown(window, { code: "F1", key: "F1" });
    fireEvent.keyDown(window, { code: "F1", key: "F1", repeat: true });
    fireEvent.keyUp(window, { code: "F1", key: "F1" });
    expect(commands.cue.mock.calls).toEqual([
      [0, true],
      [0, false],
    ]);
  });

  it("leaves Space alone while a button has focus", async () => {
    await mountApp();
    const settings = screen.getByRole("button", { name: "Settings" });
    fireEvent.keyDown(settings, { code: "Space", key: " " });
    expect(commands.togglePlaying).not.toHaveBeenCalled();
  });

  it("releases a held CUE when the window loses focus", async () => {
    await mountApp();
    fireEvent.keyDown(window, { code: "F1", key: "F1" });
    fireEvent.blur(window);
    expect(commands.cue.mock.calls).toEqual([
      [0, true],
      [0, false],
    ]);
  });
});

describe("Settings dialog", () => {
  it("lists audio fields and applies the settings", async () => {
    await mountApp();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("combobox", { name: "Host API" })).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Output device" })).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Sample rate" })).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Buffer size" })).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Master output" })).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Headphone output" })).toBeTruthy();
    expect(screen.getByTestId("latency").textContent).toContain("default buffer");
    expect(screen.getByTestId("audio-status").textContent).toContain("Running on Test Device");

    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(commands.setAudioSettings).toHaveBeenCalledTimes(1));
    expect((commands.setAudioSettings.mock.calls as unknown[][])[0]?.[0]).toMatchObject({
      master: { left: 0, right: 1 },
      headphones: { left: 2, right: 3 },
    });
    expect(await screen.findByText("Audio output restarted.")).toBeTruthy();
  });

  it("shows the error when applying fails", async () => {
    commands.setAudioSettings.mockResolvedValueOnce({
      status: "error",
      error: { kind: "audio", message: "could not open the output stream" },
    } as never);
    await mountApp();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("combobox", { name: "Host API" });
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(await screen.findByText("Audio problem: could not open the output stream")).toBeTruthy();
    expect(screen.getByRole("combobox", { name: "Host API" })).toBeTruthy();
  });
});

describe("Keyboard shortcuts for both decks", () => {
  const key = (code: string, init: KeyboardEventInit = {}, target: Window | Element = window) => {
    fireEvent.keyDown(target, { code, key: code, ...init });
    fireEvent.keyUp(target, { code, key: code, ...init });
  };

  it("Enter plays deck B and F2 holds its CUE", async () => {
    await mountApp();
    key("Enter");
    expect(commands.togglePlaying).toHaveBeenCalledWith(1);
    fireEvent.keyDown(window, { code: "F2", key: "F2" });
    fireEvent.keyDown(window, { code: "F2", key: "F2", repeat: true });
    fireEvent.keyUp(window, { code: "F2", key: "F2" });
    expect(commands.cue.mock.calls).toEqual([
      [1, true],
      [1, false],
    ]);
  });

  it("toggles sync, quantize and slip on the deck whose key was pressed", async () => {
    await mountApp();
    key("KeyS");
    expect(commands.setSync).toHaveBeenLastCalledWith(0, "beat");
    key("KeyK");
    expect(commands.setSync).toHaveBeenLastCalledWith(1, "beat");
    key("KeyP");
    expect(commands.setQuantize).toHaveBeenLastCalledWith(1, false);
    key("KeyA");
    expect(commands.setSlip).toHaveBeenLastCalledWith(0, true);
  });

  it("drives loops and beat jump", async () => {
    await mountApp();
    key("KeyZ");
    key("KeyX");
    key("KeyC");
    expect(commands.loopIn).toHaveBeenCalledWith(0);
    expect(commands.loopOut).toHaveBeenCalledWith(0);
    expect(commands.loopToggle).toHaveBeenCalledWith(0);
    key("KeyN");
    key("KeyM");
    key("Comma");
    expect(commands.loopIn).toHaveBeenLastCalledWith(1);
    expect(commands.loopOut).toHaveBeenLastCalledWith(1);
    expect(commands.loopToggle).toHaveBeenLastCalledWith(1);
    key("KeyD");
    key("KeyF");
    key("KeyH");
    key("KeyJ");
    expect(commands.beatJump.mock.calls).toEqual([
      [0, -4],
      [0, 4],
      [1, -4],
      [1, 4],
    ]);
  });

  it("sets an empty hot cue and triggers a set one, holding it while the key is down", async () => {
    await mountApp();
    key("Digit2");
    expect(commands.setHotCue).toHaveBeenCalledWith(0, 1);
    act(() =>
      bindings.emit("hot-cues-changed", {
        deck: 1,
        slots: [
          {
            id: null,
            kind: { kind: "hot", index: 0 },
            start: 3,
            end: null,
            color: null,
            name: "",
          },
        ],
      }),
    );
    fireEvent.keyDown(window, { code: "Numpad1", key: "1" });
    fireEvent.keyUp(window, { code: "Numpad1", key: "1" });
    expect(commands.triggerHotCue.mock.calls).toEqual([
      [1, 0, true],
      [1, 0, false],
    ]);
    // Shift deletes the set cue.
    key("Numpad1", { shiftKey: true });
    expect(commands.deleteHotCue).toHaveBeenCalledWith(1, 0);
  });

  it("ignores letter keys while typing but not while a button has focus", async () => {
    await mountApp();
    const input = document.createElement("input");
    input.type = "text";
    document.body.append(input);
    key("KeyS", {}, input);
    expect(commands.setSync).not.toHaveBeenCalled();
    input.remove();
    const settings = screen.getByRole("button", { name: /Audio output/ });
    key("KeyS", {}, settings);
    expect(commands.setSync).toHaveBeenCalledWith(0, "beat");
  });

  it("releases held hot cues and rolls when the window loses focus", async () => {
    await mountApp();
    act(() =>
      bindings.emit("hot-cues-changed", {
        deck: 0,
        slots: [
          {
            id: null,
            kind: { kind: "hot", index: 0 },
            start: 3,
            end: null,
            color: null,
            name: "",
          },
        ],
      }),
    );
    fireEvent.keyDown(window, { code: "Digit1", key: "1" });
    fireEvent.blur(window);
    expect(commands.triggerHotCue.mock.calls).toEqual([
      [0, 0, true],
      [0, 0, false],
    ]);
  });
});

describe("Startup", () => {
  it("reads the mixer and both decks from the backend", async () => {
    await mountApp();
    await waitFor(() => expect(commands.getMixerState).toHaveBeenCalled());
    expect(commands.getDeckState.mock.calls).toEqual([[0], [1]]);
  });

  it("shows hot cues the backend reports and clears them with the next event", async () => {
    await mountApp();
    const slot = {
      id: null,
      kind: { kind: "hot", index: 4 },
      start: 3,
      end: null,
      color: null,
      name: "",
    };
    act(() =>
      bindings.emit("hot-cues-changed", {
        deck: 1,
        slots: [null, null, null, null, slot, null, null, null],
      }),
    );
    expect(useAppStore.getState().decks[1]?.hotCues[4]).toEqual(slot);
    expect(useAppStore.getState().decks[0]?.hotCues[4]).toBeNull();
    act(() =>
      bindings.emit("hot-cues-changed", {
        deck: 1,
        slots: Array(8).fill(null),
      }),
    );
    expect(useAppStore.getState().decks[1]?.hotCues[4]).toBeNull();
  });
});
