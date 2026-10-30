import {
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  cn,
} from "@opendeck/design-system";
import {
  flexRender,
  getCoreRowModel,
  useReactTable,
  type ColumnSizingState,
  type VisibilityState,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type DragEvent,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import type { SortColumnDto } from "../ipc/bindings";
import { writeDrag } from "../lib/dnd";
import { SORT_COLUMN, displayTitle, type ColumnId } from "../lib/library";
import {
  EMPTY_SELECTION,
  clickSelect,
  contextTargets,
  focusAt,
  moveFocus,
  type SelectionState,
} from "../lib/selection";
import type { TrackSource } from "../lib/trackCache";
import {
  addToPlaylist,
  firstFreeDeck,
  loadTrackById,
  reanalyzeTracks,
  rateTrack,
  removeFromPlaylist,
  removeTracks,
  revealInExplorer,
} from "../state/libraryActions";
import { playlistsOnly, useLibraryStore } from "../state/library";
import { VISIBLE_DECKS } from "../state/app";
import { NUMERIC_COLUMNS, buildColumns, type TableRowData } from "./trackColumns";

/** Rows rendered beyond the viewport on each side. */
const OVERSCAN_ROWS = 16;

/** Row height used when the `--row-h` token cannot be read (mirrors tokens.css). */
const FALLBACK_ROW_HEIGHT = 24;

/** Rows a PageUp or PageDown moves when the viewport size is unknown. */
const FALLBACK_PAGE_ROWS = 10;

function readRowHeight(): number {
  if (typeof document === "undefined") {
    return FALLBACK_ROW_HEIGHT;
  }
  const value = Number.parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--row-h"),
  );
  return Number.isFinite(value) && value > 0 ? value : FALLBACK_ROW_HEIGHT;
}

const DECK_NAME = ["A", "B", "C", "D"];

interface TrackTableProps {
  source: TrackSource;
  /** Playlist being shown; enables "Remove from playlist". */
  playlist?: number | null;
  /** Current server sort, or `null` when the source keeps its own order. */
  sort: { column: SortColumnDto; descending: boolean } | null;
  onSort: (column: ColumnId) => void;
  label: string;
}

/**
 * The track table: TanStack Table for columns, sizing and visibility, TanStack
 * Virtual for the rows. Only the rows in view (plus a margin) are rendered and
 * requested from the source, so 50k rows scroll as smoothly as 50.
 */
export function TrackTable({ source, playlist = null, sort, onSort, label }: TrackTableProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [rowHeight] = useState(readRowHeight);
  const version = useSyncExternalStore(source.subscribe, source.getVersion);

  const selection = useLibraryStore((s) => s.selection);
  const setSelection = useLibraryStore((s) => s.setSelection);
  const notation = useLibraryStore((s) => s.keyNotation);
  const columnVisibility = useLibraryStore((s) => s.columnVisibility);
  const columnSizing = useLibraryStore((s) => s.columnSizing);
  const setColumnVisibility = useLibraryStore((s) => s.setColumnVisibility);
  const setColumnSizing = useLibraryStore((s) => s.setColumnSizing);
  const playlists = useLibraryStore((s) => s.playlists);

  const total = source.total;
  // TanStack's hooks return unmemoizable functions; this component is not compiled.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: OVERSCAN_ROWS,
    scrollPaddingStart: rowHeight,
  });
  const items = virtualizer.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items.at(-1)?.index ?? -1;

  // Ask the source for exactly what is on screen.
  useEffect(() => {
    if (last >= first) {
      source.ensureRange(first, last + 1);
    }
  }, [source, first, last, version]);

  const windowRows = items.map((item): TableRowData => source.getRow(item.index));
  const columns = useMemo(
    () =>
      buildColumns({
        notation,
        onRate: (track, stars) => {
          source.patchRow(track, { rating: stars });
          rateTrack(track, stars);
        },
      }),
    [notation, source],
  );
  const table = useReactTable<TableRowData>({
    data: windowRows,
    columns,
    getCoreRowModel: getCoreRowModel(),
    getRowId: (row, index) => (row ? String(row.id) : `loading-${first + index}`),
    columnResizeMode: "onChange",
    state: { columnVisibility, columnSizing },
    onColumnVisibilityChange: (updater) =>
      setColumnVisibility(
        typeof updater === "function" ? updater(columnVisibility as VisibilityState) : updater,
      ),
    onColumnSizingChange: (updater) =>
      setColumnSizing(
        typeof updater === "function" ? updater(columnSizing as ColumnSizingState) : updater,
      ),
  });

  const idAt = useCallback((index: number) => source.getRow(index)?.id ?? null, [source]);

  // ----- selection -----

  const select = (event: MouseEvent, index: number) => {
    const mods = { shift: event.shiftKey, ctrl: event.ctrlKey || event.metaKey };
    if (mods.shift && selection.anchor !== null) {
      const from = Math.min(selection.anchor, index);
      const to = Math.max(selection.anchor, index);
      // Rows of the range that are not loaded yet are fetched first.
      void source.fetchRange(from, to + 1).then(() => {
        setSelection(clickSelect(useLibraryStore.getState().selection, index, mods, idAt));
      });
      return;
    }
    setSelection(clickSelect(selection, index, mods, idAt));
  };

  const focusRow = (next: SelectionState) => {
    setSelection(next);
    if (next.focus !== null) {
      virtualizer.scrollToIndex(next.focus, { align: "auto" });
    }
  };

  const selectAll = () => {
    void source.fetchRange(0, total).then(() => {
      setSelection({
        ids: new Set(source.idsInRange(0, total)),
        anchor: 0,
        focus: selection.focus ?? 0,
      });
    });
  };

  const loadIndexToDeck = (index: number, deck: number) => {
    const id = idAt(index);
    if (id !== null) {
      void loadTrackById(deck, id);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) {
      return;
    }
    const pageRows = Math.max(
      1,
      Math.floor((scrollRef.current?.clientHeight ?? 0) / rowHeight) - 1 || FALLBACK_PAGE_ROWS,
    );
    const mod = event.ctrlKey || event.metaKey;
    switch (event.key) {
      case "ArrowDown":
        focusRow(moveFocus(selection, 1, total, event.shiftKey, idAt));
        break;
      case "ArrowUp":
        focusRow(moveFocus(selection, -1, total, event.shiftKey, idAt));
        break;
      case "PageDown":
        focusRow(moveFocus(selection, pageRows, total, event.shiftKey, idAt));
        break;
      case "PageUp":
        focusRow(moveFocus(selection, -pageRows, total, event.shiftKey, idAt));
        break;
      case "Home":
        focusRow(focusAt(selection, 0, total, event.shiftKey, idAt));
        break;
      case "End":
        focusRow(focusAt(selection, total - 1, total, event.shiftKey, idAt));
        break;
      case "a":
      case "A":
        if (!mod) {
          return;
        }
        selectAll();
        break;
      case "Enter":
        if (selection.focus !== null) {
          loadIndexToDeck(selection.focus, event.shiftKey ? 1 : firstFreeDeck());
        }
        break;
      case "Delete":
        if (selection.ids.size > 0) {
          setPendingDelete([...selection.ids]);
        }
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  // ----- context menu and delete confirmation -----

  const [menuTargets, setMenuTargets] = useState<number[]>([]);
  const [menuPath, setMenuPath] = useState<string | null>(null);
  const [menuEntries, setMenuEntries] = useState<number[]>([]);
  const [pendingDelete, setPendingDelete] = useState<number[] | null>(null);

  const onRowContextMenu = (index: number, row: NonNullable<TableRowData>) => {
    let next = selection;
    if (!selection.ids.has(row.id)) {
      next = clickSelect(EMPTY_SELECTION, index, { shift: false, ctrl: false }, idAt);
      setSelection(next);
    }
    const targets = contextTargets(next, row.id);
    setMenuTargets(targets);
    setMenuPath(targets.length === 1 ? row.path : null);
    setMenuEntries(entriesFor(targets));
  };

  const onDragStart = (event: DragEvent, index: number, row: NonNullable<TableRowData>) => {
    const ids = selection.ids.has(row.id) ? [...selection.ids] : [row.id];
    if (!selection.ids.has(row.id)) {
      setSelection(clickSelect(EMPTY_SELECTION, index, { shift: false, ctrl: false }, idAt));
    }
    const entries = playlist === null ? [] : entriesFor(ids);
    writeDrag(
      event.dataTransfer,
      {
        kind: "tracks",
        ids,
        ...(playlist !== null ? { playlist: { id: playlist, entries } } : {}),
      },
      ids.length === 1 ? displayTitle(row) : `${ids.length} tracks`,
    );
  };

  /** Playlist entry ids of the given tracks; empty for sources that are not playlists. */
  function entriesFor(ids: number[]): number[] {
    if (!source.entryIdAt) {
      return [];
    }
    const wanted = new Set(ids);
    const entries: number[] = [];
    for (let i = 0; i < total; i++) {
      const id = idAt(i);
      const entry = source.entryIdAt(i);
      if (id !== null && entry != null && wanted.has(id)) {
        entries.push(entry);
      }
    }
    return entries;
  }

  const paddingTop = items[0]?.start ?? 0;
  const paddingBottom = items.length > 0 ? virtualizer.getTotalSize() - (items.at(-1)?.end ?? 0) : 0;
  const visibleColumns = table.getVisibleLeafColumns().length;
  const deckIds = VISIBLE_DECKS;

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            ref={scrollRef}
            role="grid"
            aria-label={label}
            aria-rowcount={total + 1}
            aria-colcount={visibleColumns}
            aria-multiselectable
            tabIndex={0}
            onKeyDown={onKeyDown}
            className="min-h-0 flex-1 overflow-auto focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus-ring"
          >
            <Table
              className="table-fixed"
              style={{ width: table.getTotalSize(), minWidth: "100%" }}
            >
              <TableHeader>
                {table.getHeaderGroups().map((group) => (
                  <TableRow key={group.id} role="row" className="hover:bg-transparent">
                    {group.headers.map((header) => {
                      const id = header.column.id as ColumnId;
                      const sortColumn = SORT_COLUMN[id];
                      const active = sort !== null && sortColumn === sort.column;
                      return (
                        <TableHead
                          key={header.id}
                          role="columnheader"
                          aria-sort={
                            active ? (sort.descending ? "descending" : "ascending") : undefined
                          }
                          style={{ width: header.getSize() }}
                          className={cn(
                            "relative",
                            NUMERIC_COLUMNS.has(id) && "text-right",
                            sortColumn && "cursor-pointer hover:text-fg",
                          )}
                          onClick={sortColumn ? () => onSort(id) : undefined}
                        >
                          <span className="inline-flex items-center gap-1">
                            {flexRender(header.column.columnDef.header, header.getContext())}
                            {active ? (
                              sort.descending ? (
                                <ArrowDown className="size-3" aria-hidden />
                              ) : (
                                <ArrowUp className="size-3" aria-hidden />
                              )
                            ) : null}
                          </span>
                          <span
                            role="separator"
                            aria-orientation="vertical"
                            aria-label={`Resize ${String(header.column.columnDef.header)}`}
                            onMouseDown={header.getResizeHandler()}
                            onTouchStart={header.getResizeHandler()}
                            onClick={(event) => event.stopPropagation()}
                            className={cn(
                              "absolute right-0 top-0 h-full w-1 cursor-col-resize select-none touch-none hover:bg-accent",
                              header.column.getIsResizing() && "bg-accent",
                            )}
                          />
                        </TableHead>
                      );
                    })}
                  </TableRow>
                ))}
              </TableHeader>
              <TableBody>
                {paddingTop > 0 ? (
                  <tr aria-hidden style={{ height: paddingTop }}>
                    <td colSpan={visibleColumns} />
                  </tr>
                ) : null}
                {table.getRowModel().rows.map((tableRow, i) => {
                  const index = first + i;
                  const row = tableRow.original;
                  const selected = row !== null && selection.ids.has(row.id);
                  return (
                    <TableRow
                      key={tableRow.id}
                      role="row"
                      aria-rowindex={index + 2}
                      aria-selected={selected}
                      data-index={index}
                      draggable={row !== null}
                      style={{ height: rowHeight }}
                      className={cn(
                        row?.missing && "text-fg-subtle line-through",
                        selection.focus === index &&
                          "outline-2 -outline-offset-2 outline-focus-ring/40",
                      )}
                      onClick={(event) => select(event, index)}
                      onDoubleClick={() => loadIndexToDeck(index, firstFreeDeck())}
                      onContextMenu={() => row && onRowContextMenu(index, row)}
                      onDragStart={(event) => row && onDragStart(event, index, row)}
                    >
                      {tableRow.getVisibleCells().map((cell) => (
                        <TableCell
                          key={cell.id}
                          role="gridcell"
                          className={cn(NUMERIC_COLUMNS.has(cell.column.id as ColumnId) && "text-right")}
                        >
                          {flexRender(cell.column.columnDef.cell, cell.getContext())}
                        </TableCell>
                      ))}
                    </TableRow>
                  );
                })}
                {paddingBottom > 0 ? (
                  <tr aria-hidden style={{ height: paddingBottom }}>
                    <td colSpan={visibleColumns} />
                  </tr>
                ) : null}
              </TableBody>
            </Table>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          {deckIds.map((deck) => (
            <ContextMenuItem
              key={deck}
              disabled={menuTargets.length === 0}
              onSelect={() => menuTargets[0] !== undefined && void loadTrackById(deck, menuTargets[0])}
            >
              Load to deck {DECK_NAME[deck]}
            </ContextMenuItem>
          ))}
          <ContextMenuSeparator />
          <ContextMenuLabel>Add to playlist</ContextMenuLabel>
          {playlistsOnly(playlists).length === 0 ? (
            <ContextMenuItem disabled>No playlists yet</ContextMenuItem>
          ) : (
            playlistsOnly(playlists).map((p) => (
              <ContextMenuItem key={p.id} onSelect={() => void addToPlaylist(p.id, menuTargets)}>
                {p.name}
              </ContextMenuItem>
            ))
          )}
          <ContextMenuSeparator />
          <ContextMenuItem onSelect={() => void reanalyzeTracks(menuTargets)}>
            Reanalyze
          </ContextMenuItem>
          <ContextMenuItem
            disabled={menuPath === null}
            onSelect={() => menuPath !== null && revealInExplorer(menuPath)}
          >
            Reveal in explorer
          </ContextMenuItem>
          <ContextMenuSeparator />
          {playlist !== null ? (
            <ContextMenuItem onSelect={() => void removeFromPlaylist(playlist, menuEntries)}>
              Remove from playlist
            </ContextMenuItem>
          ) : null}
          <ContextMenuItem onSelect={() => setPendingDelete(menuTargets)}>
            Delete from collection
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>
      <Dialog open={pendingDelete !== null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete from collection</DialogTitle>
            <DialogDescription>
              {pendingDelete?.length === 1
                ? "This track leaves the collection and its playlists. The file stays on disk."
                : `These ${pendingDelete?.length ?? 0} tracks leave the collection and its playlists. The files stay on disk.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={() => setPendingDelete(null)}>Cancel</Button>
            <Button
              variant="danger"
              onClick={() => {
                const ids = pendingDelete ?? [];
                setPendingDelete(null);
                void removeTracks(ids);
              }}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
