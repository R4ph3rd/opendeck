import {
  EmptyState,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@opendeck/design-system";
import { History } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type SavedSetDetailDto } from "../ipc/bindings";
import { formatClock } from "../lib/library";
import { report } from "../state/actions";
import { loadTrackById } from "../state/libraryActions";

const DECK_NAME = ["A", "B", "C", "D"];

/** The tracks played in one saved set, in play order. Double-click loads a track again. */
export function SavedSetView({ id }: { id: number }) {
  const [detail, setDetail] = useState<SavedSetDetailDto | null>(null);

  useEffect(() => {
    let cancelled = false;
    void report(commands.getSavedSet(id)).then((loaded) => {
      if (!cancelled && loaded) {
        setDetail(loaded);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [id]);

  if (detail !== null && detail.entries.length === 0) {
    return (
      <EmptyState
        className="flex-1"
        icon={<History aria-hidden />}
        title="Nothing played yet"
        description="Tracks you load into a deck are added to the current set."
      />
    );
  }
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <Table aria-label={detail?.set.name ?? "Saved set"}>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead className="w-10">#</TableHead>
            <TableHead>Title</TableHead>
            <TableHead>Artist</TableHead>
            <TableHead className="w-14">Deck</TableHead>
            <TableHead className="w-16">Played</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {(detail?.entries ?? []).map((entry, i) => (
            <TableRow
              // Entries have no id; the position in the set is the key.
              key={i}
              onDoubleClick={() => {
                if (entry.track !== null) {
                  void loadTrackById(entry.deck ?? 0, entry.track);
                }
              }}
              className={entry.track === null ? "text-fg-subtle" : undefined}
            >
              <TableCell>{i + 1}</TableCell>
              <TableCell>{entry.title ?? "Untitled"}</TableCell>
              <TableCell>{entry.artist ?? ""}</TableCell>
              <TableCell>{entry.deck === null ? "" : (DECK_NAME[entry.deck] ?? "")}</TableCell>
              <TableCell>{entry.played_at === null ? "" : formatClock(entry.played_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
