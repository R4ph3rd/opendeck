import { Button, EmptyState } from "@opendeck/design-system";
import { FolderPlus, Library, SearchX } from "lucide-react";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import {
  DEFAULT_SORT,
  SORT_COLUMN,
  buildQuery,
  hasFilters,
  querySignature,
  type ColumnId,
} from "../lib/library";
import { commands, type TrackQueryDto } from "../ipc/bindings";
import { EMPTY_SELECTION } from "../lib/selection";
import { TrackWindowCache, type QueryBase } from "../lib/trackCache";
import { fetchTrackPage, pickAndImportFolder } from "../state/libraryActions";
import { useLibraryStore } from "../state/library";
import { FilterBar } from "./FilterBar";
import { TrackTable } from "./TrackTable";

/** Waits this long after the last keystroke before searching. */
export const SEARCH_DEBOUNCE_MS = 150;

/** A value that follows `value` after it has been stable for `delay` ms. */
export function useDebounced<T>(value: T, delay: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), delay);
    return () => clearTimeout(timer);
  }, [value, delay]);
  return settled;
}

/** The whole collection with server-side search, filters and sort. */
export function CollectionView() {
  const filters = useLibraryStore((s) => s.filters);
  const sort = useLibraryStore((s) => s.sort);
  const setSort = useLibraryStore((s) => s.setSort);
  const revision = useLibraryStore((s) => s.revision);
  const setSelection = useLibraryStore((s) => s.setSelection);
  const setCollectionTotal = useLibraryStore((s) => s.setCollectionTotal);

  const text = useDebounced(filters.text, SEARCH_DEBOUNCE_MS);
  // One cache per distinct query: the signature is what changes, not the objects.
  const signature = querySignature(buildQuery({ ...filters, text }, sort));
  const cache = useMemo(
    () => new TrackWindowCache(JSON.parse(signature) as QueryBase, fetchTrackPage),
    [signature],
  );
  const version = useSyncExternalStore(cache.subscribe, cache.getVersion);

  // Fetch the first page whenever the cache changes, so the empty state is
  // decided without the table (and a refresh of an empty view still refetches).
  useEffect(() => {
    cache.ensureRange(0, 1);
  }, [cache, version]);

  const seenRevision = useRef(revision);
  useEffect(() => {
    if (seenRevision.current !== revision) {
      seenRevision.current = revision;
      cache.refresh();
    }
  }, [cache, revision]);

  // The controller's browse encoder walks the list that is on screen.
  useEffect(() => {
    const query: TrackQueryDto = { ...(JSON.parse(signature) as QueryBase), offset: 0, limit: 1 };
    void commands.browseSetContext({ source: { kind: "collection", query }, index: null });
  }, [signature]);

  // The selection belongs to the rows of one query.
  useEffect(() => {
    setSelection(EMPTY_SELECTION);
  }, [cache, setSelection]);

  const filtered = hasFilters(filters);
  useEffect(() => {
    if (cache.loaded && !filtered) {
      setCollectionTotal(cache.total);
    }
  }, [cache, version, filtered, setCollectionTotal]);

  const onSort = (column: ColumnId) => {
    const target = SORT_COLUMN[column];
    if (!target) {
      return;
    }
    setSort(
      sort.column === target
        ? { column: target, descending: !sort.descending }
        : { column: target, descending: target === DEFAULT_SORT.column },
    );
  };

  const empty = cache.loaded && cache.total === 0;
  return (
    <>
      <FilterBar />
      {empty ? (
        filtered ? (
          <EmptyState
            className="flex-1"
            icon={<SearchX aria-hidden />}
            title="No tracks match"
            description="Change the search or clear the filters."
            action={
              <Button onClick={() => useLibraryStore.getState().clearFilters()}>
                Clear filters
              </Button>
            }
          />
        ) : (
          <EmptyState
            className="flex-1"
            icon={<Library aria-hidden />}
            title="Your collection is empty"
            description="Import a folder of music, or drag files and folders here from the Files tab."
            action={
              <Button variant="primary" onClick={() => void pickAndImportFolder()}>
                <FolderPlus className="size-4" aria-hidden />
                Import folder
              </Button>
            }
          />
        )
      ) : (
        <TrackTable source={cache} sort={sort} onSort={onSort} label="Collection" />
      )}
    </>
  );
}
