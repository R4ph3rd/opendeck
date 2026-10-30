import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { track } from "../test/fixtures";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("./app");
const { useLibraryStore } = await import("./library");
const { startEventBridge } = await import("./events");

const { commands } = bindings;
const ok = <T>(data: T) => Promise.resolve({ status: "ok" as const, data });

let stop = () => {};

beforeEach(async () => {
  localStorage.clear();
  useAppStore.getState().reset();
  useLibraryStore.getState().reset();
  vi.clearAllMocks();
  stop = await startEventBridge();
});

afterEach(() => stop());

const progress = (patch: Record<string, unknown> = {}) => ({
  job: 1,
  kind: "import_files",
  state: "running",
  stage: "tags",
  items_total: 4,
  items_done: 1,
  items_duplicate: 0,
  items_failed: 0,
  current_item: "/music/a.wav",
  last_track: null,
  ...patch,
});

describe("library events", () => {
  it("loads the stored jobs at startup", () => {
    expect(commands.listJobs).toHaveBeenCalled();
  });

  it("feeds import progress to the job list", () => {
    bindings.emit("import-progress", progress());
    expect(useLibraryStore.getState().jobs).toHaveLength(1);
    bindings.emit("import-progress", progress({ items_done: 3 }));
    expect(useLibraryStore.getState().jobs).toHaveLength(1);
    expect(useLibraryStore.getState().jobs[0]?.items_done).toBe(3);
  });

  it("refreshes views and the tree when the library changes", () => {
    const before = useLibraryStore.getState().revision;
    commands.listPlaylists.mockClear();
    bindings.emit("library-changed", { tracks: true, playlists: false });
    expect(useLibraryStore.getState().revision).toBe(before + 1);
    expect(commands.listPlaylists).toHaveBeenCalledTimes(1);
  });

  it("updates BPM and key on the deck that holds an analysed track", async () => {
    useAppStore.getState().patchDeck(0, { track: { ...track, id: 7, bpm: null, key: null, key_musical: null } });
    commands.getAnalysis.mockImplementation((() =>
      ok({ bpm: 128, key: "9B", key_musical: "D", beat_grid: null, grid_locked: false })) as never);
    bindings.emit("analysis-ready", 7);
    await vi.waitFor(() => expect(useAppStore.getState().decks[0]?.track?.bpm).toBe(128));
    expect(useAppStore.getState().decks[0]?.track).toMatchObject({ key: "9B", key_musical: "D" });
    // The other deck is untouched.
    expect(useAppStore.getState().decks[1]?.track).toBeNull();
  });

  it("ignores analysis of a track no deck holds", () => {
    commands.getAnalysis.mockClear();
    bindings.emit("analysis-ready", 99);
    expect(commands.getAnalysis).not.toHaveBeenCalled();
  });

  it("shows the analysis on a deck that is still loading", async () => {
    useAppStore.getState().patchDeck(1, { incoming: { ...track, id: 9, bpm: null }, loading: true });
    commands.getAnalysis.mockImplementation((() =>
      ok({ bpm: 90, key: null, key_musical: null, beat_grid: null, grid_locked: false })) as never);
    bindings.emit("analysis-ready", 9);
    await vi.waitFor(() => expect(useAppStore.getState().decks[1]?.incoming?.bpm).toBe(90));
  });
});
