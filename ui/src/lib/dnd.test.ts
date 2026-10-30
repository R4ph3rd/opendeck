import { describe, expect, it } from "vitest";
import { makeNode, makeTransfer } from "../test/library";
import {
  canDropTracks,
  dragKind,
  type DragPayload,
  flattenTree,
  hasDrag,
  intentAt,
  isWithin,
  planPlaylistMove,
  readDrag,
  writeDrag,
} from "./dnd";

// Tree: Sets (folder 1) holds Warm up (2), Peak (3); Archive (folder 4) holds Old (5); Loose (6) at top.
const nodes = [
  makeNode(1, "Sets", "folder", null, 0),
  makeNode(2, "Warm up", "playlist", 1, 0),
  makeNode(3, "Peak", "playlist", 1, 1),
  makeNode(4, "Archive", "folder", null, 1),
  makeNode(5, "Old", "playlist", 4, 0),
  makeNode(6, "Loose", "playlist", null, 2),
];

describe("planPlaylistMove", () => {
  it("drops inside a folder at the end of its children", () => {
    expect(planPlaylistMove(nodes, 6, 1, "inside")).toEqual({ parent: 1, position: 2 });
  });

  it("drops before and after a sibling at the right index", () => {
    expect(planPlaylistMove(nodes, 6, 2, "before")).toEqual({ parent: 1, position: 0 });
    expect(planPlaylistMove(nodes, 6, 2, "after")).toEqual({ parent: 1, position: 1 });
  });

  it("counts positions in the list without the moved node", () => {
    // Peak (index 1) moved before Warm up leaves one sibling after the drop point.
    expect(planPlaylistMove(nodes, 3, 2, "before")).toEqual({ parent: 1, position: 0 });
    // Warm up moved after Peak: Peak is index 0 once Warm up is lifted out, so after = 1.
    expect(planPlaylistMove(nodes, 2, 3, "after")).toEqual({ parent: 1, position: 1 });
  });

  it("moves to the top level next to a top-level node", () => {
    expect(planPlaylistMove(nodes, 2, 6, "after")).toEqual({ parent: null, position: 3 });
    expect(planPlaylistMove(nodes, 2, 1, "before")).toEqual({ parent: null, position: 0 });
  });

  it("places a drop inside a playlist after it, because playlists do not nest", () => {
    expect(planPlaylistMove(nodes, 6, 5, "inside")).toEqual({ parent: 4, position: 1 });
  });

  it("refuses a node onto itself and a folder into its own subtree", () => {
    expect(planPlaylistMove(nodes, 1, 1, "inside")).toBeNull();
    expect(planPlaylistMove(nodes, 1, 2, "inside")).toBeNull();
    expect(planPlaylistMove(nodes, 1, 3, "after")).toBeNull();
    expect(planPlaylistMove(nodes, 1, 99, "inside")).toBeNull();
  });

  it("detects ancestry", () => {
    expect(isWithin(nodes, 3, 1)).toBe(true);
    expect(isWithin(nodes, 5, 1)).toBe(false);
    expect(isWithin(nodes, 1, 1)).toBe(true);
  });
});

describe("drop zones", () => {
  it("splits a row into top edge, middle and bottom edge", () => {
    expect(intentAt(0.1)).toBe("before");
    expect(intentAt(0.5)).toBe("inside");
    expect(intentAt(0.9)).toBe("after");
  });

  it("accepts tracks on playlists only", () => {
    expect(canDropTracks(nodes[1] as never)).toBe(true);
    expect(canDropTracks(nodes[0] as never)).toBe(false);
  });
});

describe("flattenTree", () => {
  it("lists open folders with their children in sibling order", () => {
    const rows = flattenTree(nodes, new Set([1]));
    expect(rows.map((r) => [r.node.name, r.depth])).toEqual([
      ["Sets", 0],
      ["Warm up", 1],
      ["Peak", 1],
      ["Archive", 0],
      ["Loose", 0],
    ]);
  });

  it("hides the children of closed folders", () => {
    expect(flattenTree(nodes, new Set()).map((r) => r.node.name)).toEqual([
      "Sets",
      "Archive",
      "Loose",
    ]);
  });
});

describe("drag payloads", () => {
  it("round-trips a payload and exposes its kind while dragging", () => {
    const transfer = makeTransfer();
    writeDrag(transfer, { kind: "tracks", ids: [1, 2] }, "2 tracks");
    expect(hasDrag(transfer)).toBe(true);
    expect(dragKind(transfer)).toBe("tracks");
    expect(readDrag(transfer)).toEqual({ kind: "tracks", ids: [1, 2] });
    expect(transfer.getData("text/plain")).toBe("2 tracks");
  });

  it("tells the three payload kinds apart", () => {
    const payloads: DragPayload[] = [
      { kind: "files", files: [{ path: "/a", kind: "dir" }] },
      { kind: "playlist", id: 4 },
    ];
    for (const payload of payloads) {
      const transfer = makeTransfer();
      writeDrag(transfer, payload, "x");
      expect(dragKind(transfer)).toBe(payload.kind);
    }
  });

  it("ignores foreign and malformed drags", () => {
    const foreign = makeTransfer();
    foreign.setData("text/plain", "hello");
    expect(hasDrag(foreign)).toBe(false);
    expect(dragKind(foreign)).toBeNull();
    expect(readDrag(foreign)).toBeNull();

    const broken = makeTransfer();
    broken.setData("application/x-opendeck", "{not json");
    expect(readDrag(broken)).toBeNull();
    broken.setData("application/x-opendeck", JSON.stringify({ kind: "tracks" }));
    expect(readDrag(broken)).toBeNull();
  });
});
