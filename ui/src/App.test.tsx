import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

const { commands } = bindings;

beforeEach(() => {
  useAppStore.getState().reset();
  telemetryStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(() => {
  releaseAllCues();
  cleanup();
});

/** Renders the app and waits until the event bridge has subscribed. */
async function mountApp() {
  render(<App />);
  await waitFor(() => expect(bindings.events.engineTelemetry.listen).toHaveBeenCalled());
  await waitFor(() => expect(useAppStore.getState().audio).not.toBeNull());
}

describe("App", () => {
  it("lays out the top bar, two deck slots, the pads panel and the collection", async () => {
    await mountApp();
    expect(screen.getByRole("heading", { name: "OpenDeck" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Deck 1" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Deck 2" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Pads and effects" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Collection" })).toBeTruthy();
    expect(screen.getByText("No track loaded")).toBeTruthy();
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
    expect(screen.getByRole("button", { name: "PLAY" })).toHaveProperty("disabled", false);
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
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("Could not load bad.mp3: corrupt audio");
    expect(useAppStore.getState().decks[0]?.loading).toBe(false);
  });

  it("toasts a lost audio device and marks audio off", async () => {
    await mountApp();
    act(() => {
      bindings.emit("engine-event", { type: "device_lost" });
      bindings.emit("audio-device-changed", { ...runningAudio, running: false, error: "Output device lost" });
    });
    expect(screen.getByRole("alert").textContent).toContain("Audio device lost");
    expect(screen.getByRole("button", { name: /Audio output: Audio off/ })).toBeTruthy();
  });

  it("ignores engine events that need no message", async () => {
    await mountApp();
    act(() => bindings.emit("engine-event", { type: "track_ended", deck: 0 }));
    expect(screen.queryByRole("alert")).toBeNull();
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
    await waitFor(() =>
      expect(useAppStore.getState().toasts.map((t) => t.message)).toContain("Audio output restarted."),
    );
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
    await waitFor(() =>
      expect(useAppStore.getState().toasts.map((t) => t.message)).toContain(
        "could not open the output stream",
      ),
    );
    expect(screen.getByRole("combobox", { name: "Host API" })).toBeTruthy();
  });
});
