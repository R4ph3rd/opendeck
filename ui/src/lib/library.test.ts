import { describe, expect, it } from "vitest";
import { makeRow } from "../test/library";
import {
  CAMELOT_KEYS,
  DEFAULT_SORT,
  NO_FILTERS,
  SORT_COLUMN,
  buildQuery,
  columnForSort,
  displayTitle,
  formatSize,
  hasFilters,
  keyText,
} from "./library";

describe("buildQuery", () => {
  it("sends nulls and empty lists for an unfiltered collection", () => {
    expect(buildQuery(NO_FILTERS, DEFAULT_SORT)).toEqual({
      text: null,
      bpm_min: null,
      bpm_max: null,
      keys: [],
      genre: null,
      min_rating: null,
      status: null,
      sort: "date_added",
      descending: true,
    });
  });

  it("maps every filter and the sort to the request", () => {
    const query = buildQuery(
      { text: "  deep house ", bpmMin: 120, bpmMax: 128, key: "8A", genre: " Techno ", minRating: 3 },
      { column: "bpm", descending: false },
    );
    expect(query).toMatchObject({
      text: "deep house",
      bpm_min: 120,
      bpm_max: 128,
      keys: ["8A"],
      genre: "Techno",
      min_rating: 3,
      sort: "bpm",
      descending: false,
    });
  });

  it("treats blank text and a zero rating as no filter", () => {
    const query = buildQuery({ ...NO_FILTERS, text: "   ", minRating: 0 }, DEFAULT_SORT);
    expect(query.text).toBeNull();
    expect(query.min_rating).toBeNull();
  });
});

describe("column helpers", () => {
  it("round-trips every sortable column through its server sort", () => {
    for (const [column, sort] of Object.entries(SORT_COLUMN)) {
      expect(columnForSort(sort)).toBe(column);
    }
  });

  it("reports whether any filter is active", () => {
    expect(hasFilters(NO_FILTERS)).toBe(false);
    expect(hasFilters({ ...NO_FILTERS, genre: "House" })).toBe(true);
  });

  it("lists the 24 Camelot keys in wheel order", () => {
    expect(CAMELOT_KEYS).toHaveLength(24);
    expect(CAMELOT_KEYS.slice(0, 3)).toEqual(["1A", "1B", "2A"]);
  });

  it("shows the key in the chosen notation", () => {
    const row = makeRow(0, { key: "8A", key_musical: "Am" });
    expect(keyText(row, "camelot")).toBe("8A");
    expect(keyText(row, "musical")).toBe("Am");
    expect(keyText(makeRow(0, { key: null, key_musical: null }), "camelot")).toBe("");
  });

  it("falls back to the file name without extension when a track has no title", () => {
    expect(displayTitle({ title: null, path: "/music/sub/My Song.flac" })).toBe("My Song");
    expect(displayTitle({ title: "Tagged", path: "/x.wav" })).toBe("Tagged");
    expect(displayTitle({ title: null, path: "C:\\music\\a.mp3" })).toBe("a");
  });

  it("formats sizes", () => {
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(1_500_000)).toBe("1.5 MB");
  });
});
