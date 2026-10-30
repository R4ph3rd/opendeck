import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DirListing } from "../ipc/bindings";
import type { createBindingsMock } from "../test/bindingsMock";
import { makeTransfer } from "../test/library";

vi.mock("../ipc/bindings", async () => (await import("../test/bindingsMock")).createBindingsMock());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const bindings = (await import("../ipc/bindings")) as unknown as ReturnType<
  typeof createBindingsMock
>;
const { useAppStore } = await import("../state/app");
const { FileExplorer } = await import("./FileExplorer");

const { commands } = bindings;
const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const HOME: DirListing = {
  path: "/music",
  parent: "/",
  entries: [
    { name: "crate", path: "/music/crate", kind: "dir" },
    { name: "a.wav", path: "/music/a.wav", kind: "audio" },
    { name: "b.flac", path: "/music/b.flac", kind: "audio" },
    { name: "mix.m3u8", path: "/music/mix.m3u8", kind: "playlist" },
    { name: "library.xml", path: "/music/library.xml", kind: "playlist" },
  ],
};

const CRATE: DirListing = {
  path: "/music/crate",
  parent: "/music",
  entries: [{ name: "deep.mp3", path: "/music/crate/deep.mp3", kind: "audio" }],
};

const option = (name: string) => screen.getByRole("option", { name });

beforeEach(() => {
  useAppStore.getState().reset();
  vi.clearAllMocks();
  commands.listDirectory.mockImplementation(((path: string | null) =>
    ok(path === "/music/crate" ? CRATE : path === "/" ? { path: "/", parent: "", entries: [] } : HOME)) as never);
});

afterEach(cleanup);

describe("FileExplorer", () => {
  it("lists the home folder first", async () => {
    render(<FileExplorer />);
    expect(await screen.findByRole("option", { name: "crate" })).toBeTruthy();
    expect(commands.listDirectory).toHaveBeenCalledWith(null);
    expect(screen.getAllByRole("option")).toHaveLength(5);
    expect(screen.getByTitle("/music")).toBeTruthy();
  });

  it("opens a folder on double-click and goes back up", async () => {
    render(<FileExplorer />);
    fireEvent.doubleClick(await screen.findByRole("option", { name: "crate" }));
    expect(await screen.findByRole("option", { name: "deep.mp3" })).toBeTruthy();
    expect(commands.listDirectory).toHaveBeenLastCalledWith("/music/crate");
    fireEvent.click(screen.getByRole("button", { name: "Parent folder" }));
    await screen.findByRole("option", { name: "a.wav" });
    expect(commands.listDirectory).toHaveBeenLastCalledWith("/music");
  });

  it("opens a folder with Enter and goes up with Backspace", async () => {
    render(<FileExplorer />);
    const crate = await screen.findByRole("option", { name: "crate" });
    fireEvent.keyDown(crate, { key: "Enter" });
    const deep = await screen.findByRole("option", { name: "deep.mp3" });
    fireEvent.keyDown(deep, { key: "Backspace" });
    await screen.findByRole("option", { name: "a.wav" });
  });

  it("lists the roots from the drives button", async () => {
    render(<FileExplorer />);
    await screen.findByRole("option", { name: "crate" });
    fireEvent.click(screen.getByRole("button", { name: "Drives and roots" }));
    await waitFor(() => expect(commands.listDirectory).toHaveBeenLastCalledWith(""));
  });

  it("selects with click, ctrl and shift", async () => {
    render(<FileExplorer />);
    await screen.findByRole("option", { name: "crate" });
    fireEvent.click(option("a.wav"));
    fireEvent.click(option("mix.m3u8"), { ctrlKey: true });
    expect(option("a.wav").getAttribute("aria-selected")).toBe("true");
    expect(option("mix.m3u8").getAttribute("aria-selected")).toBe("true");
    expect(option("b.flac").getAttribute("aria-selected")).toBe("false");
    fireEvent.click(option("crate"));
    fireEvent.click(option("b.flac"), { shiftKey: true });
    expect(
      screen.getAllByRole("option").filter((o) => o.getAttribute("aria-selected") === "true"),
    ).toHaveLength(3);
  });

  it("loads a double-clicked audio file into the first free deck", async () => {
    render(<FileExplorer />);
    fireEvent.doubleClick(await screen.findByRole("option", { name: "a.wav" }));
    await waitFor(() => expect(commands.loadTrackPath).toHaveBeenCalledWith(0, "/music/a.wav"));
  });

  it("imports a double-clicked playlist file", async () => {
    render(<FileExplorer />);
    fireEvent.doubleClick(await screen.findByRole("option", { name: "mix.m3u8" }));
    await waitFor(() => expect(commands.importPlaylistFile).toHaveBeenCalledWith("/music/mix.m3u8", null));
    fireEvent.doubleClick(option("library.xml"));
    await waitFor(() => expect(commands.importRekordboxXml).toHaveBeenCalledWith("/music/library.xml", null));
  });

  it("drags the selection, or just the row when it is not selected", async () => {
    render(<FileExplorer />);
    await screen.findByRole("option", { name: "crate" });
    fireEvent.click(option("a.wav"));
    fireEvent.click(option("b.flac"), { ctrlKey: true });
    const together = makeTransfer();
    fireEvent.dragStart(option("b.flac"), { dataTransfer: together });
    expect(JSON.parse(together.getData("application/x-opendeck"))).toEqual({
      kind: "files",
      files: [
        { path: "/music/a.wav", kind: "audio" },
        { path: "/music/b.flac", kind: "audio" },
      ],
    });
    expect(together.getData("text/plain")).toBe("2 items");

    const alone = makeTransfer();
    fireEvent.dragStart(option("library.xml"), { dataTransfer: alone });
    expect(JSON.parse(alone.getData("application/x-opendeck"))).toEqual({
      kind: "files",
      files: [{ path: "/music/library.xml", kind: "playlist", ext: "xml" }],
    });
    // The dragged row becomes the selection.
    expect(option("library.xml").getAttribute("aria-selected")).toBe("true");
    expect(option("a.wav").getAttribute("aria-selected")).toBe("false");
  });

  it("offers deck loading and import in the context menu", async () => {
    render(<FileExplorer />);
    fireEvent.contextMenu(await screen.findByRole("option", { name: "b.flac" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Load to deck B" }));
    await waitFor(() => expect(commands.loadTrackPath).toHaveBeenCalledWith(1, "/music/b.flac"));

    fireEvent.contextMenu(option("crate"));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Import folder" }));
    await waitFor(() =>
      expect(commands.importFolder).toHaveBeenCalledWith("/music/crate", {
        features: { tempo: true, key: true, loudness: true, peaks: true },
        target_playlist: null,
      }),
    );
  });

  it("says when a folder has nothing to show", async () => {
    commands.listDirectory.mockImplementation((() =>
      ok({ path: "/empty", parent: "/", entries: [] })) as never);
    render(<FileExplorer />);
    expect(await screen.findByText("No folders or music files here.")).toBeTruthy();
  });
});
