import {
  cleanup,
  createEvent,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { Toaster, toast } from "@opendeck/design-system";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { createBindingsMock } from "../test/bindingsMock";
import { makeNode, makeTransfer } from "../test/library";
import { writeDrag, type DragPayload } from "../lib/dnd";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useLibraryStore } = await import("../state/library");
const { PlaylistTree } = await import("./PlaylistTree");

const { commands } = bindings;
const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

// Sets (folder 1) holds Warm up (2) and Peak (3); Archive (folder 4) is empty; Loose (6) is top level.
const NODES = [
  makeNode(1, "Sets", "folder", null, 0),
  makeNode(2, "Warm up", "playlist", 1, 0),
  makeNode(3, "Peak", "playlist", 1, 1),
  makeNode(4, "Archive", "folder", null, 1),
  makeNode(6, "Loose", "playlist", null, 2),
];

const SET = {
  id: 3,
  name: "Session 2026-10-30 21:15",
  notes: "",
  started_at: null,
  created_at: 0,
  updated_at: 0,
  track_count: 4,
};

/** A transfer carrying `payload` the way a drag source writes it. */
function dragging(payload: DragPayload) {
  const transfer = makeTransfer();
  writeDrag(transfer, payload, "drag");
  return transfer;
}

/** Fires a drag event with a pointer height (jsdom has no DragEvent, so `clientY` is set by hand). */
function dragAt(
  type: "dragOver" | "drop",
  el: HTMLElement,
  dataTransfer: ReturnType<typeof makeTransfer>,
  clientY: number,
) {
  const event = createEvent[type](el, { dataTransfer });
  Object.defineProperty(event, "clientY", { value: clientY });
  return fireEvent(el, event);
}

const item = (name: string) => screen.getByRole("treeitem", { name: new RegExp(name) });

async function renderTree(open: string[] = ["Sets"]) {
  render(
    <Toaster>
      <PlaylistTree />
    </Toaster>,
  );
  await screen.findByRole("treeitem", { name: /Sets/ });
  for (const folder of open) {
    fireEvent.click(item(folder));
  }
}

beforeEach(() => {
  localStorage.clear();
  useLibraryStore.getState().reset();
  vi.clearAllMocks();
  commands.listPlaylists.mockImplementation((() => ok(NODES)) as never);
  commands.listSavedSets.mockImplementation((() => ok([SET])) as never);
});

afterEach(() => {
  toast.dismiss();
  cleanup();
});

describe("PlaylistTree", () => {
  it("shows folders closed and opens them on click", async () => {
    await renderTree([]);
    expect(screen.queryByRole("treeitem", { name: /Warm up/ })).toBeNull();
    fireEvent.click(item("Sets"));
    expect(item("Warm up").getAttribute("aria-level")).toBe("2");
    expect(item("Sets").getAttribute("aria-expanded")).toBe("true");
    expect(item("Loose").getAttribute("aria-level")).toBe("1");
  });

  it("opens a playlist and a saved set in the main view", async () => {
    await renderTree();
    fireEvent.click(item("Warm up"));
    expect(useLibraryStore.getState().view).toEqual({ kind: "playlist", id: 2 });
    fireEvent.click(screen.getByRole("button", { name: /Session 2026/ }));
    expect(useLibraryStore.getState().view).toEqual({ kind: "set", id: 3 });
    fireEvent.click(screen.getByTestId("sidebar-collection"));
    expect(useLibraryStore.getState().view).toEqual({ kind: "collection" });
  });

  describe("dropping tracks", () => {
    it("adds them to the playlist they land on", async () => {
      await renderTree();
      const transfer = dragging({ kind: "tracks", ids: [5, 6] });
      expect(fireEvent.dragOver(item("Warm up"), { dataTransfer: transfer })).toBe(false);
      fireEvent.drop(item("Warm up"), { dataTransfer: transfer });
      await waitFor(() => expect(commands.addToPlaylist).toHaveBeenCalledWith(2, [5, 6], null));
    });

    it("does not accept tracks on a folder", async () => {
      await renderTree();
      const transfer = dragging({ kind: "tracks", ids: [5] });
      // dragover not cancelled means the browser refuses the drop.
      expect(fireEvent.dragOver(item("Sets"), { dataTransfer: transfer })).toBe(true);
      fireEvent.drop(item("Sets"), { dataTransfer: transfer });
      expect(commands.addToPlaylist).not.toHaveBeenCalled();
    });

    it("ignores drags that are not ours", async () => {
      await renderTree();
      const foreign = makeTransfer();
      foreign.setData("text/plain", "hello");
      expect(fireEvent.dragOver(item("Warm up"), { dataTransfer: foreign })).toBe(true);
      fireEvent.drop(item("Warm up"), { dataTransfer: foreign });
      expect(commands.addToPlaylist).not.toHaveBeenCalled();
    });
  });

  describe("dropping files from the explorer", () => {
    const files = [
      { path: "/music/a.wav", kind: "audio" as const },
      { path: "/music/crate", kind: "dir" as const },
    ];

    it("imports them into the playlist they land on", async () => {
      await renderTree();
      fireEvent.drop(item("Peak"), { dataTransfer: dragging({ kind: "files", files }) });
      await waitFor(() =>
        expect(commands.importFiles).toHaveBeenCalledWith(["/music/a.wav"], {
          features: { tempo: true, key: true, loudness: true, peaks: true },
          target_playlist: 3,
        }),
      );
      expect(commands.importFolder).toHaveBeenCalledWith("/music/crate", {
        features: { tempo: true, key: true, loudness: true, peaks: true },
        target_playlist: 3,
      });
    });

    it("imports them into the collection when dropped on the collection row", async () => {
      await renderTree();
      const transfer = dragging({ kind: "files", files });
      expect(fireEvent.dragOver(screen.getByTestId("sidebar-collection"), { dataTransfer: transfer })).toBe(
        false,
      );
      fireEvent.drop(screen.getByTestId("sidebar-collection"), { dataTransfer: transfer });
      await waitFor(() =>
        expect(commands.importFiles).toHaveBeenCalledWith(["/music/a.wav"], {
          features: { tempo: true, key: true, loudness: true, peaks: true },
          target_playlist: null,
        }),
      );
    });

    it("routes playlist files and rekordbox XML to their importers", async () => {
      await renderTree();
      fireEvent.drop(item("Peak"), {
        dataTransfer: dragging({
          kind: "files",
          files: [
            { path: "/music/mix.m3u8", kind: "playlist", ext: "m3u8" },
            { path: "/music/rb.xml", kind: "playlist", ext: "xml" },
          ],
        }),
      });
      await waitFor(() => expect(commands.importPlaylistFile).toHaveBeenCalledWith("/music/mix.m3u8", null));
      expect(commands.importRekordboxXml).toHaveBeenCalledWith("/music/rb.xml", null);
    });
  });

  describe("reordering by drag", () => {
    /** Gives a row a 20 px box so the pointer position decides before, inside or after. */
    function layout(el: HTMLElement) {
      el.getBoundingClientRect = () =>
        ({ top: 100, height: 20, bottom: 120, left: 0, right: 100, width: 100, x: 0, y: 100 }) as DOMRect;
    }

    it("moves a playlist into a folder when dropped on its middle", async () => {
      await renderTree();
      layout(item("Sets"));
      const transfer = dragging({ kind: "playlist", id: 6 });
      dragAt("dragOver", item("Sets"), transfer, 110);
      dragAt("drop", item("Sets"), transfer, 110);
      await waitFor(() => expect(commands.movePlaylist).toHaveBeenCalledWith(6, 1, 2));
    });

    it("moves before a sibling when dropped on its top edge", async () => {
      await renderTree();
      layout(item("Warm up"));
      const transfer = dragging({ kind: "playlist", id: 6 });
      dragAt("dragOver", item("Warm up"), transfer, 101);
      dragAt("drop", item("Warm up"), transfer, 101);
      await waitFor(() => expect(commands.movePlaylist).toHaveBeenCalledWith(6, 1, 0));
    });

    it("moves after a sibling when dropped on its bottom edge", async () => {
      await renderTree();
      layout(item("Peak"));
      const transfer = dragging({ kind: "playlist", id: 2 });
      dragAt("dragOver", item("Peak"), transfer, 119);
      dragAt("drop", item("Peak"), transfer, 119);
      await waitFor(() => expect(commands.movePlaylist).toHaveBeenCalledWith(2, 1, 1));
    });

    it("refuses to drop a folder into its own subtree", async () => {
      await renderTree();
      layout(item("Warm up"));
      const transfer = dragging({ kind: "playlist", id: 1 });
      dragAt("dragOver", item("Warm up"), transfer, 110);
      dragAt("drop", item("Warm up"), transfer, 110);
      expect(commands.movePlaylist).not.toHaveBeenCalled();
    });

    it("moves a playlist to the end of the top level when dropped on the empty area", async () => {
      await renderTree();
      const tree = screen.getByRole("tree", { name: "Playlists" });
      const transfer = dragging({ kind: "playlist", id: 2 });
      expect(fireEvent.dragOver(tree, { dataTransfer: transfer })).toBe(false);
      fireEvent.drop(tree, { dataTransfer: transfer });
      await waitFor(() => expect(commands.movePlaylist).toHaveBeenCalledWith(2, null, 3));
    });

    it("starts a drag that carries the playlist id", async () => {
      await renderTree();
      const transfer = makeTransfer();
      fireEvent.dragStart(item("Loose"), { dataTransfer: transfer });
      expect(JSON.parse(transfer.getData("application/x-opendeck"))).toEqual({
        kind: "playlist",
        id: 6,
      });
    });
  });

  describe("editing", () => {
    it("creates a playlist and starts renaming it", async () => {
      commands.createPlaylist.mockImplementation((() => ok(7)) as never);
      await renderTree();
      commands.listPlaylists.mockImplementation((() =>
        ok([...NODES, makeNode(7, "New playlist", "playlist", null, 3)])) as never);
      fireEvent.click(screen.getByRole("button", { name: "New playlist" }));
      await waitFor(() =>
        expect(commands.createPlaylist).toHaveBeenCalledWith(null, "New playlist", false),
      );
      // The new playlist appears in rename mode after the tree refreshes.
      const field = await screen.findByLabelText("Rename New playlist");
      fireEvent.change(field, { target: { value: "Mix" } });
      fireEvent.keyDown(field, { key: "Enter" });
      await waitFor(() => expect(commands.renamePlaylist).toHaveBeenCalledWith(7, "Mix"));
    });

    it("creates a folder inside a folder from its menu", async () => {
      commands.createPlaylist.mockImplementation((() => ok(8)) as never);
      await renderTree();
      fireEvent.contextMenu(item("Sets"));
      fireEvent.click(await screen.findByRole("menuitem", { name: "New folder" }));
      await waitFor(() =>
        expect(commands.createPlaylist).toHaveBeenCalledWith(1, "New folder", true),
      );
    });

    it("renames with Escape cancelling and Enter committing", async () => {
      await renderTree();
      fireEvent.keyDown(item("Loose"), { key: "F2" });
      const field = screen.getByLabelText("Rename Loose");
      fireEvent.keyDown(field, { key: "Escape" });
      expect(screen.queryByLabelText("Rename Loose")).toBeNull();
      expect(commands.renamePlaylist).not.toHaveBeenCalled();
      fireEvent.keyDown(item("Loose"), { key: "F2" });
      const again = screen.getByLabelText("Rename Loose");
      fireEvent.change(again, { target: { value: "Closing" } });
      fireEvent.keyDown(again, { key: "Enter" });
      await waitFor(() => expect(commands.renamePlaylist).toHaveBeenCalledWith(6, "Closing"));
    });

    it("asks before deleting a playlist", async () => {
      await renderTree();
      fireEvent.contextMenu(item("Loose"));
      fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
      expect(commands.deletePlaylist).not.toHaveBeenCalled();
      const dialog = await screen.findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
      await waitFor(() => expect(commands.deletePlaylist).toHaveBeenCalledWith(6));
    });

    it("renames and deletes a saved set from its menu", async () => {
      await renderTree();
      fireEvent.contextMenu(screen.getByRole("button", { name: /Session 2026/ }));
      fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
      const dialog = await screen.findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
      await waitFor(() => expect(commands.deleteSavedSet).toHaveBeenCalledWith(3));
    });
  });
});
