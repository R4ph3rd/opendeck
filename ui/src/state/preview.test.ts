import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { telemetry } from "../test/fixtures";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("./notify", () => ({
  notifyError: vi.fn(),
  notifySuccess: vi.fn(),
  notifyWarning: vi.fn(),
}));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { notifyError } = await import("./notify");
const { telemetryStore } = await import("./telemetry");
const { clearPreview, startPreview, stopPreview, togglePreview, usePreviewStore } =
  await import("./preview");

const { commands } = bindings;

const state = () => usePreviewStore.getState();

/** A snapshot whose preview voice reports `patch`. */
function withPreview(
  sequence: number,
  patch: Partial<ReturnType<typeof telemetry>["preview"]>,
) {
  const base = telemetry(sequence);
  return { ...base, preview: { ...base.preview, loaded: true, ...patch } };
}

beforeEach(() => {
  vi.clearAllMocks();
  telemetryStore.getState().reset();
  clearPreview();
});

afterEach(() => clearPreview());

describe("track preview state", () => {
  it("asks the backend to start at the clicked fraction and keeps the token", async () => {
    commands.previewTrack.mockResolvedValueOnce({ status: "ok", data: 42 });
    const pending = startPreview(7, 0.25);
    expect(state()).toMatchObject({ trackId: 7, token: null });
    await pending;
    expect(commands.previewTrack).toHaveBeenCalledWith(7, 0.25);
    expect(state()).toMatchObject({ trackId: 7, token: 42 });
  });

  it("clamps the fraction", async () => {
    await startPreview(7, 3);
    expect(commands.previewTrack).toHaveBeenLastCalledWith(7, 1);
    await startPreview(7, -1);
    expect(commands.previewTrack).toHaveBeenLastCalledWith(7, 0);
  });

  it("a click on the previewed track stops it, a click on another track switches", async () => {
    await startPreview(7, 0.5);
    togglePreview(8, 0.1);
    await vi.waitFor(() => expect(state()).toMatchObject({ trackId: 8, token: 1 }));
    expect(commands.previewStop).not.toHaveBeenCalled();
    togglePreview(8, 0.9);
    expect(state().trackId).toBeNull();
    expect(commands.previewStop).toHaveBeenCalledTimes(1);
    expect(commands.previewTrack).toHaveBeenCalledTimes(2);
  });

  it("ignores the answer to an older click", async () => {
    let answerFirst: (v: { status: "ok"; data: number }) => void = () => {};
    commands.previewTrack
      .mockImplementationOnce(() => new Promise((resolve) => (answerFirst = resolve)) as never)
      .mockResolvedValueOnce({ status: "ok", data: 9 });
    const first = startPreview(1, 0);
    await startPreview(2, 0);
    answerFirst({ status: "ok", data: 5 });
    await first;
    expect(state()).toMatchObject({ trackId: 2, token: 9 });
  });

  it("an error from the backend clears the state and shows a message", async () => {
    commands.previewTrack.mockResolvedValueOnce({
      status: "error",
      error: { type: "invalid", message: "file not found" },
    } as never);
    await startPreview(7, 0);
    expect(state().trackId).toBeNull();
    expect(notifyError).toHaveBeenCalled();
  });

  it("clears when the engine reports the track reached its end", async () => {
    commands.previewTrack.mockResolvedValueOnce({ status: "ok", data: 5 });
    await startPreview(7, 0);
    // An old preview ending is not ours.
    telemetryStore.getState().push(withPreview(1, { token: 4, playing: false }));
    expect(state().trackId).toBe(7);
    // Ours, not heard playing yet: still waiting for the engine.
    telemetryStore.getState().push(withPreview(2, { token: 5, playing: false }));
    expect(state().trackId).toBe(7);
    telemetryStore.getState().push(withPreview(3, { token: 5, playing: true, position: 10 }));
    expect(state()).toMatchObject({ trackId: 7, heard: true });
    telemetryStore.getState().push(withPreview(4, { token: 5, playing: false, position: 100 }));
    expect(state().trackId).toBeNull();
  });

  it("clearPreview of another track leaves the state alone", async () => {
    await startPreview(7, 0);
    clearPreview(8);
    expect(state().trackId).toBe(7);
    clearPreview(7);
    expect(state().trackId).toBeNull();
  });

  it("stopPreview tells the backend even before the start call returned", async () => {
    const pending = startPreview(7, 0);
    stopPreview();
    await pending;
    expect(state().trackId).toBeNull();
    expect(commands.previewStop).toHaveBeenCalledTimes(1);
  });
});
