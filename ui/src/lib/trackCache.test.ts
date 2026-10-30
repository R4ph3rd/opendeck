import { describe, expect, it, vi } from "vitest";
import type { TrackPageDto, TrackQueryDto } from "../ipc/bindings";
import { makeRow, pageFor } from "../test/library";
import { buildQuery, DEFAULT_SORT, NO_FILTERS } from "./library";
import { MAX_CACHED_PAGES, PAGE_SIZE, TrackWindowCache } from "./trackCache";

const TOTAL = 50_000;
const base = buildQuery(NO_FILTERS, DEFAULT_SORT);

/** A fetcher that answers from generated rows and records the requests. */
function setup(total = TOTAL) {
  const calls: TrackQueryDto[] = [];
  const fetchPage = vi.fn((query: TrackQueryDto) => {
    calls.push(query);
    return Promise.resolve<TrackPageDto | null>(pageFor(query, total));
  });
  return { calls, fetchPage, cache: new TrackWindowCache(base, fetchPage) };
}

describe("TrackWindowCache", () => {
  it("requests only the pages that cover the visible range, with offset and limit", async () => {
    const { cache, calls } = setup();
    await cache.fetchRange(250, 290);
    expect(calls).toHaveLength(1);
    expect(calls[0]).toEqual({ ...base, offset: 200, limit: PAGE_SIZE });
    expect(cache.total).toBe(TOTAL);
    expect(cache.getRow(250)?.id).toBe(251);
    // Rows of other pages are not loaded.
    expect(cache.getRow(0)).toBeNull();
    expect(cache.getRow(40_000)).toBeNull();
  });

  it("spans a range across page boundaries and loads each page once", async () => {
    const { cache, calls } = setup();
    await cache.fetchRange(90, 310);
    expect(calls.map((c) => c.offset)).toEqual([0, 100, 200, 300]);
    await cache.fetchRange(95, 105);
    expect(calls).toHaveLength(4);
  });

  it("shares a request that is already in flight", async () => {
    const { cache, fetchPage } = setup();
    cache.ensureRange(0, 10);
    cache.ensureRange(20, 30);
    await cache.fetchRange(0, 50);
    expect(fetchPage).toHaveBeenCalledTimes(1);
  });

  it("never asks past the end once the total is known", async () => {
    const { cache, calls } = setup(150);
    await cache.fetchRange(0, 10);
    await cache.fetchRange(0, 10_000);
    expect(calls.map((c) => c.offset)).toEqual([0, 100]);
    expect(cache.getRow(149)?.id).toBe(150);
    expect(cache.getRow(150)).toBeNull();
  });

  it("keeps old rows on screen while a refresh reloads them", async () => {
    const { cache, fetchPage } = setup(300);
    await cache.fetchRange(0, 100);
    const before = cache.getRow(5);
    let release: (page: TrackPageDto) => void = () => {};
    fetchPage.mockImplementationOnce(
      () => new Promise<TrackPageDto | null>((resolve) => (release = resolve)),
    );
    cache.refresh();
    expect(cache.getRow(5)).toEqual(before);
    const reload = cache.fetchRange(0, 100);
    release({ total: 301, rows: [makeRow(5, { title: "Renamed" })] });
    await reload;
    expect(cache.total).toBe(301);
    expect(cache.getRow(0)?.title).toBe("Renamed");
  });

  it("drops a page that answers for an older query generation", async () => {
    const { cache, fetchPage } = setup(300);
    let release: (page: TrackPageDto) => void = () => {};
    fetchPage.mockImplementationOnce(
      () => new Promise<TrackPageDto | null>((resolve) => (release = resolve)),
    );
    const slow = cache.fetchRange(0, 10);
    cache.refresh();
    release({ total: 5, rows: [makeRow(0)] });
    await slow;
    expect(cache.loaded).toBe(false);
    expect(cache.total).toBe(0);
  });

  it("evicts the least recently used pages past the cap", async () => {
    const { cache } = setup(PAGE_SIZE * (MAX_CACHED_PAGES + 10));
    await cache.fetchRange(0, PAGE_SIZE * (MAX_CACHED_PAGES + 5));
    expect(cache.getRow(0)).toBeNull();
    expect(cache.getRow(PAGE_SIZE * (MAX_CACHED_PAGES + 4))).not.toBeNull();
  });

  it("applies a local edit to a cached row and notifies", async () => {
    const { cache } = setup();
    await cache.fetchRange(0, 10);
    const listener = vi.fn();
    cache.subscribe(listener);
    cache.patchRow(3, { rating: 5 });
    expect(cache.getRow(2)?.rating).toBe(5);
    expect(listener).toHaveBeenCalled();
  });

  it("treats a failed page as not loaded and lets the next call retry", async () => {
    const { cache, fetchPage } = setup();
    fetchPage.mockResolvedValueOnce(null);
    await cache.fetchRange(0, 10);
    expect(cache.loaded).toBe(false);
    await cache.fetchRange(0, 10);
    expect(cache.loaded).toBe(true);
  });

  it("lists loaded ids of a range, skipping rows that are not loaded", async () => {
    const { cache } = setup();
    await cache.fetchRange(0, 100);
    expect(cache.idsInRange(98, 103)).toEqual([99, 100]);
  });
});
