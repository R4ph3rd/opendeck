import { Badge } from "@opendeck/design-system";
import { Music, Play } from "lucide-react";
import type { ReactNode } from "react";
import { useArtworkUrl } from "../hooks/useArtwork";
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
  preview: 96,
  artwork: 64,
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

/** Thumbnail of a track's artwork, or a placeholder when it has none. */
function ArtworkCell({ track }: { track: TrackRowDto }) {
  const url = useArtworkUrl(track.id);
  return (
    <span className="grid size-5 place-items-center overflow-hidden rounded-sm bg-surface-raised text-fg-subtle">
      {url ? (
        <img src={url} alt="" className="size-full object-cover" />
      ) : (
        <Music className="size-3" aria-hidden />
      )}
    </span>
  );
}

/** Preview slot. Auditioning a track from the browser needs an engine command that does not exist yet. */
function PreviewCell({ track }: { track: TrackRowDto }) {
  return (
    <button
      type="button"
      disabled
      title="Preview is not available yet"
      aria-label={`Preview ${displayTitle(track)}`}
      className="grid h-4 w-full place-items-center rounded-sm bg-surface-raised text-fg-subtle disabled:cursor-default"
    >
      <Play className="size-3" aria-hidden />
    </button>
  );
}

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
  const slot = (id: "preview" | "artwork", cell: (track: TrackRowDto) => ReactNode) =>
    helper.display({
      id,
      header: COLUMN_LABEL[id],
      size: COLUMN_SIZE[id],
      minSize: MIN_COLUMN_SIZE,
      enableSorting: false,
      cell: ({ row }) => (row.original ? cell(row.original) : null),
    });
  return [
    slot("preview", (r) => <PreviewCell track={r} />),
    slot("artwork", (r) => <ArtworkCell track={r} />),
    text("title", (r) => displayTitle(r)),
    text("artist", (r) => r.artist),
    text("album", (r) => r.album),
    text("genre", (r) => r.genre),
    text("bpm", (r) => formatBpm(r.bpm)),
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
    text("duration", (r) => (r.duration_secs === null ? "" : formatTime(r.duration_secs))),
    text("key", (r) => keyText(r, notation)),
    text("bitrate", (r) => formatBitrate(r.bitrate_kbps)),
    text("year", (r) => (r.year === null ? "" : String(r.year))),
    text("date_added", (r) => formatDate(r.date_added)),
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
