import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
  IconButton,
  cn,
} from "@opendeck/design-system";
import { ArrowUp, Folder, HardDrive, Home, ListMusic, Music } from "lucide-react";
import { useCallback, useEffect, useState, type KeyboardEvent, type MouseEvent } from "react";
import { commands, type DirListing, type FsEntry } from "../ipc/bindings";
import { writeDrag, type DraggedFile } from "../lib/dnd";
import { baseName } from "../lib/library";
import { EMPTY_SELECTION, clickSelect, type SelectionState } from "../lib/selection";
import { report } from "../state/actions";
import { VISIBLE_DECKS } from "../state/app";
import {
  firstFreeDeck,
  importDropped,
  loadDroppedFile,
  revealInExplorer,
} from "../state/libraryActions";

const DECK_NAME = ["A", "B", "C", "D"];

/** The drag payload entry for an explorer row. */
export function toDragged(entry: FsEntry): DraggedFile {
  const ext = entry.name.includes(".") ? entry.name.split(".").pop()?.toLowerCase() : undefined;
  return {
    path: entry.path,
    kind: entry.kind === "dir" ? "dir" : entry.kind === "audio" ? "audio" : "playlist",
    ...(entry.kind === "playlist" && ext ? { ext } : {}),
  };
}

const ICON = { dir: Folder, audio: Music, playlist: ListMusic } as const;

/** Browses folders on disk. Drag audio, folders and playlists to the collection or a deck. */
export function FileExplorer() {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [selection, setSelection] = useState<SelectionState>(EMPTY_SELECTION);

  const go = useCallback(async (path: string | null) => {
    const next = await report(commands.listDirectory(path));
    if (next) {
      setListing(next);
      setSelection(EMPTY_SELECTION);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void report(commands.listDirectory(null)).then((first) => {
      if (first && !cancelled) {
        setListing(first);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const entries = listing?.entries ?? [];
  const idAt = (index: number) => (index < entries.length ? index : null);

  const draggedFor = (index: number): DraggedFile[] => {
    const picked = selection.ids.has(index) ? [...selection.ids] : [index];
    return picked.flatMap((i) => (entries[i] ? [toDragged(entries[i])] : []));
  };

  const onClick = (event: MouseEvent, index: number) =>
    setSelection(
      clickSelect(
        selection,
        index,
        { shift: event.shiftKey, ctrl: event.ctrlKey || event.metaKey },
        idAt,
      ),
    );

  const open = (entry: FsEntry) => {
    if (entry.kind === "dir") {
      void go(entry.path);
    } else if (entry.kind === "audio") {
      // Load into the first deck without a track, like a double-click in the collection.
      void loadDroppedFile(firstFreeDeck(), toDragged(entry));
    } else {
      void importDropped([toDragged(entry)]);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>, index: number, entry: FsEntry) => {
    if (event.key === "Enter") {
      event.preventDefault();
      open(entry);
    } else if (event.key === "Backspace" && listing?.parent != null) {
      event.preventDefault();
      void go(listing.parent);
    } else if (event.key === " ") {
      event.preventDefault();
      setSelection(clickSelect(selection, index, { shift: false, ctrl: true }, idAt));
    }
  };

  const importSelection = (index: number) => void importDropped(draggedFor(index));

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1">
        <IconButton
          size="sm"
          aria-label="Parent folder"
          disabled={listing?.parent == null}
          onClick={() => listing?.parent != null && void go(listing.parent)}
        >
          <ArrowUp className="size-3.5" aria-hidden />
        </IconButton>
        <IconButton size="sm" aria-label="Home folder" onClick={() => void go(null)}>
          <Home className="size-3.5" aria-hidden />
        </IconButton>
        <IconButton size="sm" aria-label="Drives and roots" onClick={() => void go("")}>
          <HardDrive className="size-3.5" aria-hidden />
        </IconButton>
        <span
          className="min-w-0 flex-1 truncate text-xs text-fg-muted"
          title={listing?.path || "Computer"}
        >
          {listing ? listing.path || "Computer" : "Loading..."}
        </span>
      </div>
      <div
        role="listbox"
        aria-label="Files"
        aria-multiselectable
        className="min-h-0 flex-1 overflow-auto p-1"
      >
        {listing && entries.length === 0 ? (
          <p className="px-2 py-2 text-xs text-fg-subtle">No folders or music files here.</p>
        ) : null}
        {entries.map((entry, index) => {
          const Icon = ICON[entry.kind];
          const selected = selection.ids.has(index);
          return (
            <ContextMenu key={entry.path}>
              <ContextMenuTrigger asChild>
                <div
                  role="option"
                  tabIndex={0}
                  aria-selected={selected}
                  draggable
                  title={entry.path}
                  className={cn(
                    "flex h-[var(--control-md)] items-center gap-1.5 rounded-sm px-2 text-sm text-fg-muted",
                    "hover:bg-surface-hover hover:text-fg focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus-ring",
                    selected && "bg-accent-muted text-fg",
                  )}
                  onClick={(event) => onClick(event, index)}
                  onDoubleClick={() => open(entry)}
                  onKeyDown={(event) => onKeyDown(event, index, entry)}
                  onContextMenu={() => {
                    if (!selected) {
                      setSelection(
                        clickSelect(EMPTY_SELECTION, index, { shift: false, ctrl: false }, idAt),
                      );
                    }
                  }}
                  onDragStart={(event) => {
                    const files = draggedFor(index);
                    if (!selected) {
                      setSelection(
                        clickSelect(EMPTY_SELECTION, index, { shift: false, ctrl: false }, idAt),
                      );
                    }
                    writeDrag(
                      event.dataTransfer,
                      { kind: "files", files },
                      files.length === 1 ? entry.name : `${files.length} items`,
                    );
                  }}
                >
                  <Icon className="size-3.5 shrink-0" aria-hidden />
                  <span className="truncate">{entry.name}</span>
                </div>
              </ContextMenuTrigger>
              <ContextMenuContent>
                {entry.kind === "audio"
                  ? VISIBLE_DECKS.map((deck) => (
                      <ContextMenuItem
                        key={deck}
                        onSelect={() => void loadDroppedFile(deck, toDragged(entry))}
                      >
                        Load to deck {DECK_NAME[deck]}
                      </ContextMenuItem>
                    ))
                  : null}
                {entry.kind === "audio" ? <ContextMenuSeparator /> : null}
                <ContextMenuItem onSelect={() => importSelection(index)}>
                  {entry.kind === "dir"
                    ? "Import folder"
                    : entry.kind === "playlist"
                      ? "Import playlist"
                      : "Import to collection"}
                </ContextMenuItem>
                <ContextMenuItem onSelect={() => revealInExplorer(entry.path)}>
                  Reveal in explorer
                </ContextMenuItem>
                {entry.kind === "dir" ? (
                  <ContextMenuItem onSelect={() => void go(entry.path)}>
                    Open {baseName(entry.path)}
                  </ContextMenuItem>
                ) : null}
              </ContextMenuContent>
            </ContextMenu>
          );
        })}
      </div>
    </div>
  );
}
