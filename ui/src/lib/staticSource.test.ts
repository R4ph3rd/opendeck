import { describe, expect, it, vi } from "vitest";
import { makeRow } from "../test/library";
import { StaticTrackSource, compareRows, matchesText } from "./staticSource";

const entries = [
  { entryId: 11, row: makeRow(0, { title: "Charlie", bpm: 128, key: "8A", artist: "Zed" }) },
  { entryId: 12, row: makeRow(1, { title: "alpha", bpm: null, key: "10B", artist: "Amy" }) },
  { entryId: 13, row: makeRow(2, { title: "Bravo", bpm: 124, key: "2A", artist: null }) },
];

describe("StaticTrackSource", () => {
  it("keeps the playlist order without a sort", () => {
    const source = new StaticTrackSource(entries, null, "");
    expect(source.total).toBe(3);
    expect([0, 1, 2].map((i) => source.getRow(i)?.title)).toEqual(["Charlie", "alpha", "Bravo"]);
    expect(source.entryIdAt(1)).toBe(12);
  });

  it("sorts titles without regard to case", () => {
    const source = new StaticTrackSource(entries, { column: "title", descending: false }, "");
    expect([0, 1, 2].map((i) => source.getRow(i)?.title)).toEqual(["alpha", "Bravo", "Charlie"]);
  });

  it("puts missing values last in both directions", () => {
    const up = new StaticTrackSource(entries, { column: "bpm", descending: false }, "");
    expect([0, 1, 2].map((i) => source(up, i))).toEqual([124, 128, null]);
    const down = new StaticTrackSource(entries, { column: "bpm", descending: true }, "");
    expect([0, 1, 2].map((i) => source(down, i))).toEqual([128, 124, null]);
  });

  it("sorts keys by Camelot number, then letter", () => {
    const keys = new StaticTrackSource(entries, { column: "key", descending: false }, "");
    expect([0, 1, 2].map((i) => keys.getRow(i)?.key)).toEqual(["2A", "8A", "10B"]);
  });

  it("filters by every word over the visible text", () => {
    expect(new StaticTrackSource(entries, null, "bra").total).toBe(1);
    expect(new StaticTrackSource(entries, null, "amy alpha").total).toBe(1);
    expect(new StaticTrackSource(entries, null, "amy bravo").total).toBe(0);
    expect(new StaticTrackSource(entries, null, "  ").total).toBe(3);
  });

  it("applies and announces a local edit", () => {
    const s = new StaticTrackSource(entries, null, "");
    const listener = vi.fn();
    s.subscribe(listener);
    s.patchRow(1, { rating: 4 });
    expect(s.getRow(0)?.rating).toBe(4);
    expect(listener).toHaveBeenCalledTimes(1);
    s.patchRow(999, { rating: 1 });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("lists ids of a range", () => {
    expect(new StaticTrackSource(entries, null, "").idsInRange(1, 3)).toEqual([2, 3]);
  });

  it("exposes the comparators", () => {
    const [first, second] = entries.map((e) => e.row);
    if (!first || !second) throw new Error("fixture rows");
    expect(compareRows({ column: "rating", descending: true })(first, second)).toBe(
      first.rating > second.rating ? -1 : 1,
    );
    expect(matchesText(first, "charlie")).toBe(true);
  });
});

function source(s: StaticTrackSource, i: number): number | null {
  return s.getRow(i)?.bpm ?? null;
}
