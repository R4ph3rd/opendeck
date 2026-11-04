import type {
  AnalysisStatusDto,
  SortColumnDto,
  TrackRowDto,
} from "../ipc/bindings";
import type { QueryBase } from "./trackCache";

/** Key notation shown in the table and on the decks. */
export type KeyNotation = "camelot" | "musical";

/** Ids of the collection table columns. */
export type ColumnId =
  | "preview"
  | "artwork"
  | "title"
  | "artist"
  | "album"
  | "genre"
  | "bpm"
  | "rating"
  | "duration"
  | "key"
  | "bitrate"
  | "year"
  | "date_added"
  | "comment"
  | "analysis";

/** Every column in display order. */
export const COLUMN_ORDER: readonly ColumnId[] = [
  "preview",
  "artwork",
  "title",
  "artist",
  "album",
  "genre",
  "bpm",
  "rating",
  "duration",
  "key",
  "bitrate",
  "year",
  "date_added",
  "comment",
  "analysis",
];

/** Header text per column. */
export const COLUMN_LABEL: Record<ColumnId, string> = {
  preview: "Preview",
  artwork: "Artwork",
  title: "Track Title",
  artist: "Artist",
  album: "Album",
  bpm: "BPM",
  key: "Key",
  duration: "Time",
  bitrate: "Bitrate",
  year: "Year",
  date_added: "Added",
  genre: "Genre",
  rating: "Rating",
  comment: "Comment",
  analysis: "Analysis",
};

/** Server sort column behind each sortable column; `comment` cannot be sorted. */
export const SORT_COLUMN: Partial<Record<ColumnId, SortColumnDto>> = {
  title: "title",
  artist: "artist",
  album: "album",
  bpm: "bpm",
  key: "key",
  duration: "duration",
  bitrate: "bitrate",
  year: "year",
  date_added: "date_added",
  genre: "genre",
  rating: "rating",
  analysis: "analysis_status",
};

/** Column behind a server sort column. */
export function columnForSort(sort: SortColumnDto): ColumnId {
  for (const id of COLUMN_ORDER) {
    if (SORT_COLUMN[id] === sort) {
      return id;
    }
  }
  return "date_added";
}

/** Filters the collection header edits. `null` and empty mean "any". */
export interface TrackFilters {
  text: string;
  bpmMin: number | null;
  bpmMax: number | null;
  /** Camelot key such as `8A`, or `null` for any. */
  key: string | null;
  genre: string;
  /** Minimum stars, 0 for any. */
  minRating: number;
}

export const NO_FILTERS: TrackFilters = {
  text: "",
  bpmMin: null,
  bpmMax: null,
  key: null,
  genre: "",
  minRating: 0,
};

export interface TrackSort {
  column: SortColumnDto;
  descending: boolean;
}

/** Newest first, the order of a fresh collection. */
export const DEFAULT_SORT: TrackSort = { column: "date_added", descending: true };

/** Builds the `query_tracks` argument (minus paging) for the current filters and sort. */
export function buildQuery(filters: TrackFilters, sort: TrackSort): QueryBase {
  const text = filters.text.trim();
  const genre = filters.genre.trim();
  return {
    text: text === "" ? null : text,
    bpm_min: filters.bpmMin,
    bpm_max: filters.bpmMax,
    keys: filters.key ? [filters.key] : [],
    genre: genre === "" ? null : genre,
    min_rating: filters.minRating > 0 ? filters.minRating : null,
    status: null,
    sort: sort.column,
    descending: sort.descending,
  };
}

/** True when any filter narrows the collection. */
export function hasFilters(filters: TrackFilters): boolean {
  return JSON.stringify(filters) !== JSON.stringify(NO_FILTERS);
}

/** Stable text for a query, to tell whether two bases ask for the same rows. */
export function querySignature(query: QueryBase): string {
  return JSON.stringify(query);
}

/** The 24 Camelot keys in wheel order, for the key filter. */
export const CAMELOT_KEYS: readonly string[] = Array.from({ length: 12 }, (_, i) => [
  `${i + 1}A`,
  `${i + 1}B`,
]).flat();

const KB = 1000;

export function formatBitrate(kbps: number | null): string {
  return kbps === null ? "" : `${kbps} kbps`;
}

export function formatBpm(bpm: number | null): string {
  return bpm === null ? "" : bpm.toFixed(1);
}

/** Key text for the chosen notation; empty when the track has no key. */
export function keyText(
  row: Pick<TrackRowDto, "key" | "key_musical">,
  notation: KeyNotation,
): string {
  return (notation === "camelot" ? row.key : row.key_musical) ?? "";
}

/** `YYYY-MM-DD` in local time for a Unix ms timestamp. */
export function formatDate(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `HH:MM` in local time for a Unix ms timestamp. */
export function formatClock(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export const ANALYSIS_LABEL: Record<AnalysisStatusDto, string> = {
  pending: "Pending",
  running: "Analysing",
  done: "Done",
  failed: "Failed",
  stale: "Outdated",
};

/** File name of a path with either separator. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Title shown for a row: the tag, or the file name without extension. */
export function displayTitle(row: Pick<TrackRowDto, "title" | "path">): string {
  if (row.title) {
    return row.title;
  }
  const name = baseName(row.path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(0, dot) : name;
}

/** Approximate size text for a byte count. */
export function formatSize(bytes: number): string {
  if (bytes < KB) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / KB;
  let unit = 0;
  while (value >= KB && unit < units.length - 1) {
    value /= KB;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}
