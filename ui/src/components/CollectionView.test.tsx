import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Toaster, toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { makeRow, mockViewport, pageFor } from "../test/library";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("../state/app");
const { useLibraryStore } = await import("../state/library");
const { CollectionView } = await import("./CollectionView");

const { commands } = bindings;
const TOTAL = 50_000;
const VIEWPORT = 240;
const ROW = 24;

let restoreViewport = () => {};

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

function serve(total = TOTAL) {
  commands.queryTracks.mockImplementation(((query: Parameters<typeof pageFor>[0]) =>
    ok(pageFor(query, total))) as never);
}

type Query = Parameters<typeof pageFor>[0];

/** Every `query_tracks` argument so far, oldest first. */
const queries = () => (commands.queryTracks.mock.calls as unknown as [Query][]).map(([q]) => q);
const lastQuery = () => queries().at(-1);

const bodyRows = () =>
  screen.queryAllByRole("row").filter((r) => r.getAttribute("aria-rowindex") !== null);

beforeEach(() => {
  localStorage.clear();
  useLibraryStore.getState().reset();
  useAppStore.getState().reset();
  vi.clearAllMocks();
  restoreViewport = mockViewport(VIEWPORT);
  serve();
});

afterEach(() => {
  restoreViewport();
  toast.dismiss();
  cleanup();
});

describe("CollectionView table", () => {
  it("renders a window of a 50,000 row collection, not every row", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    const rendered = bodyRows().length;
    // Viewport rows plus the overscan on each side, far below the total.
    expect(rendered).toBeLessThan(VIEWPORT / ROW + 40);
    expect(screen.getByRole("grid", { name: "Collection" }).getAttribute("aria-rowcount")).toBe(
      String(TOTAL + 1),
    );
    expect(screen.getByText("Track 0")).toBeTruthy();
    expect(screen.queryByText("Track 40000")).toBeNull();
  });

  it("asks the server for the first page of the default newest-first order", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(commands.queryTracks).toHaveBeenCalled());
    expect(queries()[0]).toEqual({
      text: null,
      bpm_min: null,
      bpm_max: null,
      keys: [],
      genre: null,
      min_rating: null,
      status: null,
      sort: "date_added",
      descending: true,
      offset: 0,
      limit: 100,
    });
  });

  it("loads further pages only when the view scrolls to them", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    expect(queries().every((q) => q.offset < 200)).toBe(true);
    const grid = screen.getByRole("grid", { name: "Collection" });
    grid.scrollTop = ROW * 30_000;
    fireEvent.scroll(grid);
    await waitFor(() =>
      expect(queries().some((q) => q.offset >= 29_000)).toBe(true),
    );
    await waitFor(() => expect(screen.getByText("Track 30000")).toBeTruthy());
  });

  it("sorts on the server when a header is clicked, then flips the direction", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    fireEvent.click(screen.getByRole("columnheader", { name: /BPM/ }));
    await waitFor(() => expect(lastQuery()).toMatchObject({ sort: "bpm", descending: false, offset: 0 }));
    fireEvent.click(screen.getByRole("columnheader", { name: /BPM/ }));
    await waitFor(() => expect(lastQuery()).toMatchObject({ sort: "bpm", descending: true }));
    expect(screen.getByRole("columnheader", { name: /BPM/ }).getAttribute("aria-sort")).toBe(
      "descending",
    );
  });

  it("does not sort a column the server cannot sort", async () => {
    useLibraryStore.getState().setColumnVisibility({});
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    const calls = commands.queryTracks.mock.calls.length;
    fireEvent.click(screen.getByRole("columnheader", { name: /Comment/ }));
    expect(commands.queryTracks.mock.calls.length).toBe(calls);
  });

  it("sends the search text after the typing pause", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    fireEvent.change(screen.getByLabelText("Search collection"), { target: { value: "deep" } });
    fireEvent.change(screen.getByLabelText("Search collection"), { target: { value: "deep house" } });
    await waitFor(() => expect(lastQuery()).toMatchObject({ text: "deep house", offset: 0 }));
    // One query for the finished text, none for the intermediate one.
    const texts = queries().map((q) => q.text);
    expect(texts).not.toContain("deep");
  });

  it("sends the BPM range and the rating filter", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    fireEvent.change(screen.getByLabelText("Minimum BPM"), { target: { value: "120" } });
    fireEvent.change(screen.getByLabelText("Maximum BPM"), { target: { value: "128" } });
    act(() => useLibraryStore.getState().setFilters({ key: "8A", genre: "House", minRating: 3 }));
    await waitFor(() =>
      expect(lastQuery()).toMatchObject({
        bpm_min: 120,
        bpm_max: 128,
        keys: ["8A"],
        genre: "House",
        min_rating: 3,
      }),
    );
  });

  it("shows the key in the chosen notation and persists the choice", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    expect(within(bodyRows()[0] as HTMLElement).getByText("8A")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Key notation/ }));
    await waitFor(() => expect(within(bodyRows()[0] as HTMLElement).getByText("Am")).toBeTruthy());
    expect(localStorage.getItem("opendeck.library.v1")).toContain('"musical"');
  });

  it("hides columns the user turned off and keeps the title", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    expect(screen.queryByRole("columnheader", { name: /Album/ })).toBeNull();
    act(() => useLibraryStore.getState().setColumnVisibility({ album: true, bpm: false }));
    expect(screen.getByRole("columnheader", { name: /Album/ })).toBeTruthy();
    expect(screen.queryByRole("columnheader", { name: /BPM/ })).toBeNull();
    expect(screen.getByRole("columnheader", { name: /Title/ })).toBeTruthy();
  });
});

describe("CollectionView selection and actions", () => {
  async function ready() {
    render(
      <Toaster>
        <CollectionView />
      </Toaster>,
    );
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
  }

  const row = (i: number): HTMLElement => {
    const found = bodyRows().find((r) => r.getAttribute("data-index") === String(i));
    if (!found) {
      throw new Error(`row ${i} is not rendered`);
    }
    return found;
  };

  it("selects with click, ctrl and shift", async () => {
    await ready();
    fireEvent.click(row(1));
    expect(row(1).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(row(3), { ctrlKey: true });
    expect(row(1).getAttribute("aria-selected")).toBe("true");
    expect(row(3).getAttribute("aria-selected")).toBe("true");
    expect(row(2).getAttribute("aria-selected")).toBe("false");
    fireEvent.click(row(5), { shiftKey: true, ctrlKey: true });
    await waitFor(() => expect(row(4).getAttribute("aria-selected")).toBe("true"));
    expect(useLibraryStore.getState().selection.ids.size).toBe(4);
    fireEvent.click(row(0));
    expect(useLibraryStore.getState().selection.ids.size).toBe(1);
  });

  it("moves the selection with the arrow keys and loads with Enter", async () => {
    await ready();
    const grid = screen.getByRole("grid", { name: "Collection" });
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect([...useLibraryStore.getState().selection.ids]).toEqual([2]);
    fireEvent.keyDown(grid, { key: "ArrowDown", shiftKey: true });
    expect(useLibraryStore.getState().selection.ids.size).toBe(2);
    fireEvent.keyDown(grid, { key: "Enter" });
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(0, 3));
    fireEvent.keyDown(grid, { key: "Enter", shiftKey: true });
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(1, 3));
  });

  it("selects every row with ctrl+A, fetching the pages it needs", async () => {
    serve(250);
    await ready();
    fireEvent.keyDown(screen.getByRole("grid", { name: "Collection" }), { key: "a", ctrlKey: true });
    await waitFor(() => expect(useLibraryStore.getState().selection.ids.size).toBe(250));
  });

  it("loads a track into the first free deck on double-click", async () => {
    await ready();
    fireEvent.doubleClick(row(2));
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(0, 3));
    act(() => useAppStore.getState().patchDeck(0, { track: { path: "/x", file_name: "x" } as never }));
    fireEvent.doubleClick(row(4));
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(1, 5));
  });

  it("opens the context menu on the clicked row and loads it to deck B", async () => {
    await ready();
    fireEvent.contextMenu(row(2));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Load to deck B" }));
    await waitFor(() => expect(commands.loadTrack).toHaveBeenCalledWith(1, 3));
    // The right-clicked row became the selection.
    expect([...useLibraryStore.getState().selection.ids]).toEqual([3]);
  });

  it("adds the selection to a playlist from the context menu", async () => {
    act(() =>
      useLibraryStore.getState().setPlaylists([
        { id: 9, parent: null, kind: "playlist", name: "Warm up", position: 0, track_count: 0 },
      ]),
    );
    await ready();
    fireEvent.click(row(0));
    fireEvent.click(row(1), { shiftKey: true });
    await waitFor(() => expect(useLibraryStore.getState().selection.ids.size).toBe(2));
    fireEvent.contextMenu(row(1));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Warm up" }));
    await waitFor(() => expect(commands.addToPlaylist).toHaveBeenCalledWith(9, [1, 2], null));
  });

  it("reanalyzes and reveals from the context menu", async () => {
    await ready();
    fireEvent.contextMenu(row(0));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Reanalyze" }));
    await waitFor(() =>
      expect(commands.reanalyzeTracks).toHaveBeenCalledWith([1], {
        tempo: true,
        key: true,
        loudness: true,
        peaks: true,
      }),
    );
    fireEvent.contextMenu(row(0));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Reveal in explorer" }));
    await waitFor(() => expect(commands.revealInExplorer).toHaveBeenCalledWith("/music/track-0.wav"));
  });

  it("asks before deleting from the collection", async () => {
    await ready();
    fireEvent.contextMenu(row(0));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete from collection" }));
    expect(commands.deleteTracks).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: "Delete" }));
    await waitFor(() => expect(commands.deleteTracks).toHaveBeenCalledWith([1]));
  });

  it("edits the rating from the stars without selecting the row", async () => {
    await ready();
    const stars = within(row(0)).getByRole("group", { name: /Rating of Track 0/ });
    fireEvent.click(within(stars).getByRole("button", { name: "4 stars" }));
    await waitFor(() => expect(commands.setRating).toHaveBeenCalledWith(1, 4));
    expect(row(0).getAttribute("aria-selected")).toBe("false");
    // Clicking the current rating clears it.
    fireEvent.click(within(row(0)).getByRole("button", { name: "4 stars" }));
    await waitFor(() => expect(commands.setRating).toHaveBeenCalledWith(1, 0));
  });

  it("starts a drag with every selected track", async () => {
    await ready();
    fireEvent.click(row(0));
    fireEvent.click(row(2), { ctrlKey: true });
    const { makeTransfer } = await import("../test/library");
    const transfer = makeTransfer();
    fireEvent.dragStart(row(2), { dataTransfer: transfer });
    expect(JSON.parse(transfer.getData("application/x-opendeck"))).toEqual({
      kind: "tracks",
      ids: [1, 3],
    });
    expect(transfer.getData("text/plain")).toBe("2 tracks");
  });
});

describe("CollectionView empty states", () => {
  it("shows the first-run state with an import button", async () => {
    serve(0);
    render(<CollectionView />);
    expect(await screen.findByText("Your collection is empty")).toBeTruthy();
    expect(screen.getByRole("button", { name: /Import folder/ })).toBeTruthy();
  });

  it("explains an empty search and offers to clear it", async () => {
    commands.queryTracks.mockImplementation(((query: Parameters<typeof pageFor>[0]) =>
      ok(pageFor(query, query.text ? 0 : 20))) as never);
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    fireEvent.change(screen.getByLabelText("Search collection"), { target: { value: "zzz" } });
    expect(await screen.findByText("No tracks match")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
  });

  it("refetches the visible rows when the library changes", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    const before = commands.queryTracks.mock.calls.length;
    commands.queryTracks.mockImplementation(((query: Parameters<typeof pageFor>[0]) =>
      ok({
        total: 3,
        rows: [makeRow(0, { title: "Fresh import" })].slice(0, query.limit),
      })) as never);
    act(() => useLibraryStore.getState().bumpRevision());
    await waitFor(() => expect(commands.queryTracks.mock.calls.length).toBeGreaterThan(before));
    await waitFor(() => expect(screen.getByText("Fresh import")).toBeTruthy());
  });

  it("tells the controller's browse encoder what is on screen and follows its cursor", async () => {
    render(<CollectionView />);
    await waitFor(() => expect(bodyRows().length).toBeGreaterThan(5));
    await waitFor(() => expect(commands.browseSetContext).toHaveBeenCalled());
    const context = (commands.browseSetContext.mock.calls as unknown as [
      { source: { kind: string; query: { limit: number } }; index: number | null },
    ][])[0]?.[0];
    expect(context?.source.kind).toBe("collection");
    expect(context?.index).toBeNull();

    // The encoder moves to row 3 (track id 4): the table selects it.
    act(() => useLibraryStore.getState().moveBrowseCursor(3, 4));
    await waitFor(() => expect(useLibraryStore.getState().selection.focus).toBe(3));
    expect([...useLibraryStore.getState().selection.ids]).toEqual([4]);
    expect(commands.browseSelect).not.toHaveBeenCalledWith(3);

    // A click on another row is reported, so the encoder continues from there.
    const row = bodyRows().find((r) => r.getAttribute("aria-rowindex") === "8");
    expect(row).toBeTruthy();
    fireEvent.click(row as HTMLElement);
    await waitFor(() => expect(commands.browseSelect).toHaveBeenCalledWith(6));
  });
});
