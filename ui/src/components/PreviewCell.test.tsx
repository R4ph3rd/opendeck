import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { telemetry } from "../test/fixtures";
import { makeRow } from "../test/library";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("../state/notify", () => ({
  notifyError: vi.fn(),
  notifySuccess: vi.fn(),
  notifyWarning: vi.fn(),
}));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { PEAK_POINT_LEN, encodePeaks } = await import("../viz/peaks");
const { miniPeaks } = await import("../lib/miniPeaks");
const { usePreviewStore, clearPreview } = await import("../state/preview");
const { telemetryStore } = await import("../state/telemetry");
const { PreviewCell, barsPath } = await import("./PreviewCell");

const { commands } = bindings;

/** An overview blob: 16 points ramping up. */
function blob(): ArrayBuffer {
  const points = new Uint8Array(16 * PEAK_POINT_LEN);
  for (let i = 0; i < 16; i += 1) {
    points[i * PEAK_POINT_LEN] = 16 * (i + 1) - 1;
  }
  return encodePeaks({ sampleRate: 48_000, framesPerPoint: 48_000, count: 16, points })
    .buffer as ArrayBuffer;
}

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

beforeEach(() => {
  vi.clearAllMocks();
  miniPeaks.clear();
  clearPreview();
  telemetryStore.getState().reset();
  commands.getTrackPeaks.mockImplementation((() => ok(blob())) as never);
});

afterEach(cleanup);

/** The button, once the row has asked for its peaks and drawn them. */
const waveform = () => screen.findByRole("button", { name: /^Preview Track/ });

describe("PreviewCell", () => {
  it("shows an empty slot, and asks for nothing, until the track is analysed", () => {
    for (const status of ["pending", "running", "failed"] as const) {
      const { unmount } = render(<PreviewCell track={makeRow(0, { analysis_status: status })} />);
      expect(screen.getByRole("img", { name: /No preview for Track 0/ })).toBeTruthy();
      expect(screen.queryByRole("button")).toBeNull();
      unmount();
    }
    expect(commands.getTrackPeaks).not.toHaveBeenCalled();
  });

  it("fetches the lowest-resolution peaks of an analysed track and draws them", async () => {
    render(<PreviewCell track={makeRow(0, { id: 11 })} />);
    // Settles first: a row that scrolls past quickly never asks.
    expect(commands.getTrackPeaks).not.toHaveBeenCalled();
    const button = await waveform();
    expect(commands.getTrackPeaks).toHaveBeenCalledTimes(1);
    expect(commands.getTrackPeaks).toHaveBeenCalledWith(11, 3);
    const path = button.querySelector("path");
    expect(path?.getAttribute("d")?.startsWith("M0.5 ")).toBe(true);
  });

  it("fetches nothing for a row that unmounts before it settles", async () => {
    const { unmount } = render(<PreviewCell track={makeRow(0)} />);
    unmount();
    await new Promise((r) => setTimeout(r, 200));
    expect(commands.getTrackPeaks).not.toHaveBeenCalled();
  });

  it("draws an outdated analysis too, and an empty answer keeps the placeholder", async () => {
    render(<PreviewCell track={makeRow(0, { analysis_status: "stale" })} />);
    await waveform();
    cleanup();
    miniPeaks.clear();
    commands.getTrackPeaks.mockImplementation((() => ok(new ArrayBuffer(0))) as never);
    render(<PreviewCell track={makeRow(1)} />);
    await waitFor(() => expect(commands.getTrackPeaks).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByRole("img", { name: /No preview/ })).toBeTruthy();
  });

  it("starts the preview at the clicked position and does not reach the row", async () => {
    const row = vi.fn();
    render(
      <div onClick={row} onDoubleClick={row} onPointerDown={row}>
        <PreviewCell track={makeRow(0, { id: 5 })} />
      </div>,
    );
    const button = await waveform();
    button.getBoundingClientRect = () =>
      ({ left: 100, width: 200, top: 0, height: 16, right: 300, bottom: 16 }) as DOMRect;
    fireEvent.pointerDown(button);
    fireEvent.click(button, { clientX: 150, detail: 1 });
    fireEvent.doubleClick(button);
    expect(commands.previewTrack).toHaveBeenCalledWith(5, 0.25);
    expect(row).not.toHaveBeenCalled();
    await waitFor(() => expect(button.getAttribute("aria-pressed")).toBe("true"));
  });

  it("stops on a second click and starts at the beginning from the keyboard", async () => {
    render(<PreviewCell track={makeRow(0, { id: 5 })} />);
    const button = await waveform();
    button.getBoundingClientRect = () => ({ left: 0, width: 100 }) as DOMRect;
    fireEvent.click(button, { clientX: 40, detail: 1 });
    await waitFor(() => expect(usePreviewStore.getState().trackId).toBe(5));
    fireEvent.click(button, { clientX: 70, detail: 1 });
    expect(commands.previewStop).toHaveBeenCalledTimes(1);
    expect(button.getAttribute("aria-pressed")).toBe("false");
    // Enter or Space click with no pointer position.
    fireEvent.click(button, { clientX: 0, detail: 0 });
    expect(commands.previewTrack).toHaveBeenLastCalledWith(5, 0);
  });

  it("moves the playhead with the engine telemetry and only for its own token", async () => {
    commands.previewTrack.mockImplementation((() => ok(7)) as never);
    render(<PreviewCell track={makeRow(0, { id: 5 })} />);
    const button = await waveform();
    fireEvent.click(button, { clientX: 0, detail: 0 });
    await waitFor(() => expect(usePreviewStore.getState().token).toBe(7));
    const head = () => button.querySelector<HTMLElement>("span.bg-playhead");
    await waitFor(() => expect(head()).not.toBeNull());
    // Another track's preview: no playhead.
    act(() => {
      const t = telemetry(1);
      telemetryStore.getState().push({
        ...t,
        preview: { loaded: true, playing: true, token: 3, position: 50, duration: 100 },
      });
    });
    await new Promise((r) => setTimeout(r, 50));
    expect(head()?.style.visibility).toBe("hidden");
    act(() => {
      const t = telemetry(2);
      telemetryStore.getState().push({
        ...t,
        preview: { loaded: true, playing: true, token: 7, position: 25, duration: 100 },
      });
    });
    await waitFor(() => expect(head()?.style.left).toBe("25%"));
    expect(head()?.style.visibility).toBe("visible");
  });
});

describe("barsPath", () => {
  it("draws one mirrored bar per column, never thinner than a hairline", () => {
    const d = barsPath(new Uint8Array([0, 255]));
    expect(d).toBe("M0.5 48V52M1.5 2V98");
  });
});
