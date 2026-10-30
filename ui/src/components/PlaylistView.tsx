import { EmptyState } from "@opendeck/design-system";
import { ListMusic } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { commands, type PlaylistEntryDto } from "../ipc/bindings";
import { SORT_COLUMN, type ColumnId, type TrackSort } from "../lib/library";
import { StaticTrackSource } from "../lib/staticSource";
import { report } from "../state/actions";
import { useLibraryStore } from "../state/library";
import { SEARCH_DEBOUNCE_MS, useDebounced } from "./CollectionView";
import { FilterBar } from "./FilterBar";
import { TrackTable } from "./TrackTable";

/** The tracks of one playlist, in playlist order until a column is clicked. */
export function PlaylistView({ id, name }: { id: number; name: string }) {
  const revision = useLibraryStore((s) => s.revision);
  const playlists = useLibraryStore((s) => s.playlists);
  const filters = useLibraryStore((s) => s.filters);
  const text = useDebounced(filters.text, SEARCH_DEBOUNCE_MS);
  const [entries, setEntries] = useState<PlaylistEntryDto[] | null>(null);
  const [sort, setSort] = useState<TrackSort | null>(null);

  useEffect(() => {
    let cancelled = false;
    void report(commands.getPlaylistTracks(id)).then((loaded) => {
      if (!cancelled && loaded) {
        setEntries(loaded);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [id, revision, playlists]);

  const source = useMemo(
    () =>
      new StaticTrackSource(
        (entries ?? []).map((e) => ({ entryId: e.entry_id, row: e.track })),
        sort,
        text,
      ),
    [entries, sort, text],
  );

  const onSort = (column: ColumnId) => {
    const target = SORT_COLUMN[column];
    if (!target) {
      return;
    }
    // Ascending, then descending, then back to playlist order.
    if (sort?.column !== target) {
      setSort({ column: target, descending: false });
    } else if (!sort.descending) {
      setSort({ column: target, descending: true });
    } else {
      setSort(null);
    }
  };

  return (
    <>
      <FilterBar textOnly />
      {entries !== null && entries.length === 0 ? (
        <EmptyState
          className="flex-1"
          icon={<ListMusic aria-hidden />}
          title={`${name} is empty`}
          description="Drag tracks here from the collection, or use Add to playlist in the track menu."
        />
      ) : (
        <TrackTable source={source} playlist={id} sort={sort} onSort={onSort} label={name} />
      )}
    </>
  );
}
