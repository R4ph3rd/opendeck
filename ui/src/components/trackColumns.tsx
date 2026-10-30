import { Badge } from "@opendeck/design-system";
import { createColumnHelper, type ColumnDef } from "@tanstack/react-table";
import type { TrackRowDto } from "../ipc/bindings";
import { formatTime } from "../lib/format";
import {
  ANALYSIS_LABEL,
  COLUMN_LABEL,
  displayTitle,
  formatBitrate,
  formatBpm,
  formatDate,
  keyText,
  type ColumnId,
  type KeyNotation,
} from "../lib/library";
import { StarRating } from "./StarRating";

/** A table row: a track, or `null` while its page is loading. */
export type TableRowData = TrackRowDto | null;

/** Starting width in pixels of each column. */
export const COLUMN_SIZE: Record<ColumnId, number> = {
  title: 260,
  artist: 180,
  album: 160,
  bpm: 64,
  key: 56,
  duration: 64,
  bitrate: 84,
  year: 56,
  date_added: 92,
  genre: 120,
  rating: 92,
  comment: 180,
  analysis: 88,
};

/** Narrowest a column can be dragged to. */
export const MIN_COLUMN_SIZE = 40;

/** Columns whose values are numbers, right-aligned. */
export const NUMERIC_COLUMNS: ReadonlySet<ColumnId> = new Set([
  "bpm",
  "duration",
  "bitrate",
  "year",
]);

/** Columns that cannot be hidden. */
export const REQUIRED_COLUMNS: ReadonlySet<ColumnId> = new Set(["title"]);

const helper = createColumnHelper<TableRowData>();

interface ColumnOptions {
  notation: KeyNotation;
  onRate: (track: number, stars: number) => void;
}

const dim = <span className="text-fg-subtle">...</span>;

/** Column definitions for the collection table. */
export function buildColumns({ notation, onRate }: ColumnOptions): ColumnDef<TableRowData>[] {
  const text = (id: ColumnId, pick: (row: TrackRowDto) => string | null | undefined) =>
    helper.display({
      id,
      header: COLUMN_LABEL[id],
      size: COLUMN_SIZE[id],
      minSize: MIN_COLUMN_SIZE,
      cell: ({ row }) => (row.original ? (pick(row.original) ?? "") : id === "title" ? dim : null),
    });
  return [
    text("title", (r) => displayTitle(r)),
    text("artist", (r) => r.artist),
    text("album", (r) => r.album),
    text("bpm", (r) => formatBpm(r.bpm)),
    text("key", (r) => keyText(r, notation)),
    text("duration", (r) => (r.duration_secs === null ? "" : formatTime(r.duration_secs))),
    text("bitrate", (r) => formatBitrate(r.bitrate_kbps)),
    text("year", (r) => (r.year === null ? "" : String(r.year))),
    text("date_added", (r) => formatDate(r.date_added)),
    text("genre", (r) => r.genre),
    helper.display({
      id: "rating",
      header: COLUMN_LABEL.rating,
      size: COLUMN_SIZE.rating,
      minSize: MIN_COLUMN_SIZE,
      cell: ({ row }) =>
        row.original ? (
          <StarRating
            value={row.original.rating}
            label={`Rating of ${displayTitle(row.original)}`}
            onChange={(stars) => row.original && onRate(row.original.id, stars)}
          />
        ) : null,
    }),
    text("comment", (r) => r.comment),
    helper.display({
      id: "analysis",
      header: COLUMN_LABEL.analysis,
      size: COLUMN_SIZE.analysis,
      minSize: MIN_COLUMN_SIZE,
      cell: ({ row }) => {
        const status = row.original?.analysis_status;
        if (!status) {
          return null;
        }
        const variant =
          status === "done" ? "success" : status === "failed" ? "danger" : "neutral";
        return <Badge variant={variant}>{ANALYSIS_LABEL[status]}</Badge>;
      },
    }),
  ] as ColumnDef<TableRowData>[];
}
