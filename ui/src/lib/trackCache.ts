import type { TrackPageDto, TrackQueryDto, TrackRowDto } from "../ipc/bindings";

/** Rows per `query_tracks` request. */
export const PAGE_SIZE = 100;

/** Pages kept in memory; the least recently used go first. */
export const MAX_CACHED_PAGES = 60;

/** The filter and sort part of a query; the cache adds `offset` and `limit`. */
export type QueryBase = Omit<TrackQueryDto, "offset" | "limit">;

/** What the track table reads rows from: a windowed server cache or a fixed list. */
export interface TrackSource {
  /** Rows matching the filter. */
  readonly total: number;
  /** False until the first page has arrived. */
  readonly loaded: boolean;
  /** Changes on every update, for `useSyncExternalStore`. */
  getVersion(): number;
  subscribe(listener: () => void): () => void;
  /** The row at `index`, `null` while it loads. */
  getRow(index: number): TrackRowDto | null;
  /** Requests the rows `[start, end)`. */
  ensureRange(start: number, end: number): void;
  /** Resolves when every row of `[start, end)` has settled. */
  fetchRange(start: number, end: number): Promise<void>;
  /** Loaded row ids in `[start, end)`. */
  idsInRange(start: number, end: number): number[];
  /** Applies a local edit to a cached row. */
  patchRow(id: number, patch: Partial<TrackRowDto>): void;
  /** Playlist entry id of the row at `index`, for sources that show a playlist. */
  entryIdAt?(index: number): number | null;
}

/** Fetches one page; resolves `null` when the request failed (already reported). */
export type PageFetcher = (query: TrackQueryDto) => Promise<TrackPageDto | null>;

/**
 * Windowed, server-side view of the collection. The table asks for the rows it
 * shows (`ensureRange`); missing pages are requested once, in flight requests
 * are shared and a page that arrives for an older query is dropped. `refresh`
 * keeps the old rows on screen until their replacements arrive, so a library
 * change during an import does not blank the table.
 */
export class TrackWindowCache implements TrackSource {
  private pages = new Map<number, TrackRowDto[]>();
  private stale = new Map<number, TrackRowDto[]>();
  private inflight = new Map<number, Promise<void>>();
  private listeners = new Set<() => void>();
  private generation = 0;
  private version = 0;
  private known = false;
  private rows = 0;

  constructor(
    readonly base: QueryBase,
    private readonly fetchPage: PageFetcher,
    readonly pageSize: number = PAGE_SIZE,
  ) {}

  /** Rows matching the filter; 0 until the first page arrives. */
  get total(): number {
    return this.rows;
  }

  /** True once a page has told us the total. */
  get loaded(): boolean {
    return this.known;
  }

  /** Changes on every update; for `useSyncExternalStore`. */
  getVersion = (): number => this.version;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** The row at `index`, from the newest page or a stale one; `null` while it loads. */
  getRow(index: number): TrackRowDto | null {
    const page = Math.floor(index / this.pageSize);
    const rows = this.pages.get(page) ?? this.stale.get(page);
    return rows?.[index - page * this.pageSize] ?? null;
  }

  /** Requests the pages covering `[start, end)` that are not loaded or loading. */
  ensureRange(start: number, end: number): void {
    void this.fetchRange(start, end);
  }

  /** Like `ensureRange`, resolving when every page of the range has settled. */
  fetchRange(start: number, end: number): Promise<void> {
    const last = this.known ? Math.min(end, this.rows) : end;
    if (last <= start) {
      return this.known ? Promise.resolve() : this.request(0);
    }
    const first = Math.floor(Math.max(0, start) / this.pageSize);
    const lastPage = Math.floor((last - 1) / this.pageSize);
    const waits: Promise<void>[] = [];
    for (let page = first; page <= lastPage; page++) {
      if (this.pages.has(page)) {
        this.touch(page);
        continue;
      }
      waits.push(this.request(page));
    }
    return Promise.all(waits).then(() => undefined);
  }

  /** Loaded row ids in `[start, end)`, skipping rows that are not loaded. */
  idsInRange(start: number, end: number): number[] {
    const ids: number[] = [];
    for (let i = Math.max(0, start); i < Math.min(end, this.rows); i++) {
      const row = this.getRow(i);
      if (row) {
        ids.push(row.id);
      }
    }
    return ids;
  }

  /** Drops every page but shows the old rows until new ones arrive. */
  refresh(): void {
    this.generation += 1;
    for (const [page, rows] of this.pages) {
      this.stale.set(page, rows);
    }
    this.pages.clear();
    this.inflight.clear();
    this.bump();
  }

  /** Applies a local edit to a cached row (for example a new rating). */
  patchRow(id: number, patch: Partial<TrackRowDto>): void {
    let touched = false;
    for (const store of [this.pages, this.stale]) {
      for (const rows of store.values()) {
        const at = rows.findIndex((r) => r.id === id);
        const current = rows[at];
        if (current) {
          rows[at] = { ...current, ...patch };
          touched = true;
        }
      }
    }
    if (touched) {
      this.bump();
    }
  }

  private touch(page: number): void {
    const rows = this.pages.get(page);
    if (rows) {
      this.pages.delete(page);
      this.pages.set(page, rows);
    }
  }

  private request(page: number): Promise<void> {
    const pending = this.inflight.get(page);
    if (pending) {
      return pending;
    }
    const generation = this.generation;
    const query: TrackQueryDto = {
      ...this.base,
      offset: page * this.pageSize,
      limit: this.pageSize,
    };
    const job = this.fetchPage(query)
      .then((result) => {
        if (generation !== this.generation) {
          return;
        }
        this.inflight.delete(page);
        if (result) {
          this.store(page, result);
        }
      })
      .catch(() => {
        if (generation === this.generation) {
          this.inflight.delete(page);
        }
      });
    this.inflight.set(page, job);
    return job;
  }

  private store(page: number, result: TrackPageDto): void {
    this.rows = result.total;
    this.known = true;
    this.pages.set(page, result.rows);
    this.stale.delete(page);
    while (this.pages.size > MAX_CACHED_PAGES) {
      const oldest = this.pages.keys().next().value;
      if (oldest === undefined) {
        break;
      }
      this.pages.delete(oldest);
    }
    // Stale pages past the new end can never be shown again.
    const pages = Math.ceil(this.rows / this.pageSize);
    for (const key of this.stale.keys()) {
      if (key >= pages) {
        this.stale.delete(key);
      }
    }
    this.bump();
  }

  private bump(): void {
    this.version += 1;
    for (const listener of this.listeners) {
      listener();
    }
  }
}
