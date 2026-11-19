import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { neutralSampler } from "../test/bindingsMock";
import { runningAudio, track } from "../test/fixtures";
import { makeTransfer, pointerDrag } from "../test/library";
import { startPointerDrag } from "../lib/pointerDrag";
import { writeDrag, type DragPayload } from "../lib/dnd";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("../state/app");
const { useLibraryStore } = await import("../state/library");
const { telemetryStore } = await import("../state/telemetry");
const { usePadStore } = await import("../state/pads");
const { DeckPanel } = await import("./DeckPanel");

const { commands } = bindings;

const dragging = (payload: DragPayload) => {
  const transfer = makeTransfer();
  writeDrag(transfer, payload, "drag");
  return transfer;
};

const deck = (n: number) => screen.getByRole("region", { name: `Deck ${n}` });

beforeEach(() => {
  localStorage.clear();
  useAppStore.getState().reset();
  useAppStore.getState().setAudio(runningAudio);
  useLibraryStore.getState().reset();
  telemetryStore.getState().reset();
  usePadStore.getState().reset();
  vi.clearAllMocks();
});

afterEach(cleanup);

describe("loading a deck by drag and drop", () => {
  it("loads the first dragged collection track", async () => {
    render(<DeckPanel deck={1} />);
    const transfer = dragging({ kind: "tracks", ids: [41, 42] });
    expect(fireEvent.dragOver(deck(2), { dataTransfer: transfer })).toBe(false);
    fireEvent.drop(deck(2), { dataTransfer: transfer });
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(1, 41));
  });

  it("loads a dragged audio file by path, which imports it first", async () => {
    render(<DeckPanel deck={0} />);
    fireEvent.drop(deck(1), {
      dataTransfer: dragging({
        kind: "files",
        files: [
          { path: "/music/crate", kind: "dir" },
          { path: "/music/new.wav", kind: "audio" },
        ],
      }),
    });
    await waitFor(() => expect(commands.loadTrackPath).toHaveBeenCalledWith(0, "/music/new.wav"));
    expect(commands.loadTrack).not.toHaveBeenCalled();
  });

  it("does not take folders or playlist files", () => {
    render(<DeckPanel deck={0} />);
    const transfer = dragging({
      kind: "files",
      files: [{ path: "/music/crate", kind: "dir" }],
    });
    fireEvent.drop(deck(1), { dataTransfer: transfer });
    expect(commands.loadTrackPath).not.toHaveBeenCalled();
    expect(commands.loadTrack).not.toHaveBeenCalled();
  });

  it("ignores a playlist being dragged across the deck", () => {
    render(<DeckPanel deck={0} />);
    const transfer = dragging({ kind: "playlist", id: 3 });
    expect(fireEvent.dragOver(deck(1), { dataTransfer: transfer })).toBe(true);
    fireEvent.drop(deck(1), { dataTransfer: transfer });
    expect(commands.loadTrack).not.toHaveBeenCalled();
  });
});

describe("analysis on the deck", () => {
  it("shows the analysed key in the chosen notation", () => {
    act(() => useAppStore.getState().patchDeck(0, { track }));
    render(<DeckPanel deck={0} />);
    expect(screen.getByTestId("key-0").textContent).toBe("8A");
    act(() => useLibraryStore.getState().setKeyNotation("musical"));
    expect(screen.getByTestId("key-0").textContent).toBe("Am");
  });

  it("shows a dash while the track has no key", () => {
    act(() => useAppStore.getState().patchDeck(0, { track: { ...track, key: null, key_musical: null } }));
    render(<DeckPanel deck={0} />);
    expect(screen.getByTestId("key-0").textContent).toBe("--");
  });
});

describe("loading a deck by pointer drag", () => {
  // A stand-in for a track row: presses start a drag carrying `payload`.
  const source = (payload: DragPayload) => (
    <button type="button" onPointerDown={(e) => startPointerDrag(e, () => ({ payload, label: "drag" }))}>
      row
    </button>
  );

  it.each([1, 2])("loads a track dropped on deck %i", async (n) => {
    render(
      <>
        {source({ kind: "tracks", ids: [7] })}
        <DeckPanel deck={n - 1} />
      </>,
    );
    act(() => pointerDrag(screen.getByRole("button", { name: "row" }), deck(n)));
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(n - 1, 7));
  });

  it("loads a track dropped on a control inside the deck", async () => {
    render(
      <>
        {source({ kind: "tracks", ids: [8] })}
        <DeckPanel deck={1} />
      </>,
    );
    const inside = screen.getByRole("button", { name: "PLAY" });
    act(() => pointerDrag(screen.getByRole("button", { name: "row" }), inside));
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(1, 8));
  });

  it("loads a sampler slot, not the deck, when dropped on a sampler pad", () => {
    act(() => {
      usePadStore.getState().setSampler(neutralSampler() as never);
      useAppStore.getState().setPadMode(1, "sampler");
    });
    render(
      <>
        {source({ kind: "tracks", ids: [9] })}
        <DeckPanel deck={1} />
      </>,
    );
    const pad = screen.getByLabelText("Sample slot 11 (empty)");
    act(() => pointerDrag(screen.getByRole("button", { name: "row" }), pad));
    expect(commands.samplerLoadTrack).toHaveBeenCalledWith(10, 9);
    expect(commands.loadTrack).not.toHaveBeenCalled();
  });

  it("does nothing when the drag ends outside a deck", () => {
    render(
      <>
        {source({ kind: "tracks", ids: [7] })}
        <DeckPanel deck={0} />
      </>,
    );
    act(() => pointerDrag(screen.getByRole("button", { name: "row" }), document.body));
    expect(commands.loadTrack).not.toHaveBeenCalled();
  });
});
