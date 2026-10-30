import type { TrackRowDto } from "../ipc/bindings";
import { columnForSort, displayTitle, type TrackSort } from "./library";
import type { TrackSource } from "./trackCache";

/** A row of a fixed list: the track and, in a playlist, its entry id. */
export interface StaticEntry {
  entryId: number | null;
  row: TrackRowDto;
}

const text = (value: string | null): string => (value ?? "").toLowerCase();

/** Compares two rows by a sort column. Missing values sort last in both directions. */
export function compareRows(sort: TrackSort): (a: TrackRowDto, b: TrackRowDto) => number {
  const column = columnForSort(sort.column);
  const sign = sort.descending ? -1 : 1;
  const key = (r: TrackRowDto): string | number | null => {
    switch (column) {
      case "title":
        return text(displayTitle(r));
      case "artist":
        return text(r.artist) || null;
      case "album":
        return text(r.album) || null;
      case "genre":
        return text(r.genre) || null;
      case "bpm":
        return r.bpm;
      case "key": {
        // Camelot order: number, then A before B.
        const match = /^(\d+)([AB])$/.exec(r.key ?? "");
        return match ? Number(match[1]) * 2 + (match[2] === "B" ? 1 : 0) : null;
      }
      case "duration":
        return r.duration_secs;
      case "bitrate":
        return r.bitrate_kbps;
      case "year":
        return r.year;
      case "rating":
        return r.rating;
      case "analysis":
        return r.analysis_status;
      default:
        return r.date_added;
    }
  };
  return (a, b) => {
    const x = key(a);
    const y = key(b);
    if (x === null || y === null) {
      return x === y ? 0 : x === null ? 1 : -1;
    }
    return x < y ? -sign : x > y ? sign : 0;
  };
}

/** Whether a row matches free text over its visible text fields. */
export function matchesText(row: TrackRowDto, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const hay = [row.title, row.artist, row.album, row.genre, row.comment, displayTitle(row)]
    .map(text)
    .join(" ");
  return words.every((w) => hay.includes(w));
}

/**
 * A playlist or any other fixed list as a `TrackSource`. Sorting and text
 * search run here; the list is already in memory. With no sort the entries keep
 * their order (the playlist order).
 */
export class StaticTrackSource implements TrackSource {
  private rows: StaticEntry[];
  private version = 0;
  private listeners = new Set<() => void>();

  constructor(entries: readonly StaticEntry[], sort: TrackSort | null, query: string) {
    let rows = entries.filter((e) => query.trim() === "" || matchesText(e.row, query));
    if (sort) {
      const cmp = compareRows(sort);
      rows = [...rows].sort((a, b) => cmp(a.row, b.row));
    }
    this.rows = rows.map((e) => ({ ...e }));
  }

  get total(): number {
    return this.rows.length;
  }

  get loaded(): boolean {
    return true;
  }

  getVersion = (): number => this.version;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  getRow(index: number): TrackRowDto | null {
    return this.rows[index]?.row ?? null;
  }

  entryIdAt(index: number): number | null {
    return this.rows[index]?.entryId ?? null;
  }

  ensureRange(): void {}

  fetchRange(): Promise<void> {
    return Promise.resolve();
  }

  idsInRange(start: number, end: number): number[] {
    return this.rows.slice(Math.max(0, start), Math.max(0, end)).map((e) => e.row.id);
  }

  patchRow(id: number, patch: Partial<TrackRowDto>): void {
    let touched = false;
    for (const entry of this.rows) {
      if (entry.row.id === id) {
        entry.row = { ...entry.row, ...patch };
        touched = true;
      }
    }
    if (touched) {
      this.version += 1;
      for (const l of this.listeners) {
        l();
      }
    }
  }
}
