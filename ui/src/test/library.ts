import type { PlaylistNodeDto, TrackPageDto, TrackQueryDto, TrackRowDto } from "../ipc/bindings";

/** A collection row with predictable fields. */
export function makeRow(i: number, patch: Partial<TrackRowDto> = {}): TrackRowDto {
  return {
    id: i + 1,
    path: `/music/track-${i}.wav`,
    title: `Track ${i}`,
    artist: `Artist ${i % 7}`,
    album: null,
    genre: "House",
    year: 2020,
    comment: null,
    duration_secs: 180 + i,
    bitrate_kbps: 320,
    bpm: 120 + (i % 20),
    key: "8A",
    key_musical: "Am",
    rating: i % 6,
    color: null,
    date_added: 1_700_000_000_000 + i,
    analysis_status: "done",
    missing: false,
    artwork_id: null,
    ...patch,
  };
}

/** Answers a `query_tracks` call from a collection of `total` generated rows. */
export function pageFor(query: TrackQueryDto, total: number): TrackPageDto {
  const count = Math.max(0, Math.min(query.limit, total - query.offset));
  return {
    total,
    rows: Array.from({ length: count }, (_, i) => makeRow(query.offset + i)),
  };
}

export function makeNode(
  id: number,
  name: string,
  kind: "playlist" | "folder",
  parent: number | null,
  position: number,
): PlaylistNodeDto {
  return { id, name, kind, parent, position, track_count: 0 };
}

/** Pretends every element is `height` by `width` pixels (jsdom has no layout). */
export function mockViewport(height: number, width = 800): () => void {
  const proto = Element.prototype;
  const h = Object.getOwnPropertyDescriptor(proto, "clientHeight");
  const w = Object.getOwnPropertyDescriptor(proto, "clientWidth");
  Object.defineProperty(proto, "clientHeight", { configurable: true, get: () => height });
  Object.defineProperty(proto, "clientWidth", { configurable: true, get: () => width });
  return () => {
    if (h) Object.defineProperty(proto, "clientHeight", h);
    else Reflect.deleteProperty(proto, "clientHeight");
    if (w) Object.defineProperty(proto, "clientWidth", w);
    else Reflect.deleteProperty(proto, "clientWidth");
  };
}

/** A `DataTransfer` stand-in for drag and drop tests. */
export function makeTransfer() {
  const data = new Map<string, string>();
  return {
    setData: (format: string, value: string) => void data.set(format, value),
    getData: (format: string) => data.get(format) ?? "",
    get types() {
      return [...data.keys()];
    },
    effectAllowed: "all",
    dropEffect: "none",
  };
}
