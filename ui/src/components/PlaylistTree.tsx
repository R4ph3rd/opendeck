import {
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  IconButton,
  Input,
  cn,
} from "@opendeck/design-system";
import {
  ChevronDown,
  ChevronRight,
  Folder,
  FolderOpen,
  FolderPlus,
  History,
  Library,
  ListMusic,
  Plus,
} from "lucide-react";
import { useEffect, useState, type DragEvent, type KeyboardEvent } from "react";
import type { PlaylistNodeDto, SavedSetDto } from "../ipc/bindings";
import {
  canDropTracks,
  dragKind,
  flattenTree,
  intentAt,
  planPlaylistMove,
  readDrag,
  writeDrag,
  type DropIntent,
} from "../lib/dnd";
import {
  addToPlaylist,
  createPlaylist,
  deletePlaylist,
  deleteSavedSet,
  importDropped,
  movePlaylist,
  refreshPlaylists,
  refreshSavedSets,
  renamePlaylist,
  renameSavedSet,
} from "../state/libraryActions";
import { useLibraryStore, type LibraryView } from "../state/library";

const INDENT_REM = 0.75;

interface DropMark {
  /** Node id, or `"collection"` for the collection row. */
  id: number | "collection";
  intent: DropIntent;
}

/** Row styling shared by the collection, playlist and set rows. */
const rowClass = (active: boolean, mark: string | null) =>
  cn(
    "group relative flex h-[var(--control-md)] w-full items-center gap-1.5 rounded-sm px-2 text-left text-sm text-fg-muted",
    "hover:bg-surface-hover hover:text-fg focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus-ring",
    active && "bg-accent-muted text-fg",
    mark === "inside" && "bg-accent-muted outline-2 -outline-offset-2 outline-accent",
    mark === "before" && "before:absolute before:inset-x-0 before:top-0 before:h-0.5 before:bg-accent",
    mark === "after" && "after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-accent",
  );

function isView(view: LibraryView, kind: LibraryView["kind"], id?: number): boolean {
  return view.kind === kind && (id === undefined || ("id" in view && view.id === id));
}

interface RenameFieldProps {
  initial: string;
  label: string;
  onCommit: (name: string) => void;
  onCancel: () => void;
}

function RenameField({ initial, label, onCommit, onCancel }: RenameFieldProps) {
  const [value, setValue] = useState(initial);
  const commit = () => {
    const name = value.trim();
    if (name === "" || name === initial) {
      onCancel();
    } else {
      onCommit(name);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    event.stopPropagation();
    if (event.key === "Enter") {
      commit();
    } else if (event.key === "Escape") {
      onCancel();
    }
  };
  return (
    <Input
      autoFocus
      aria-label={label}
      className="h-5 px-1"
      value={value}
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={onKeyDown}
      onBlur={commit}
      onFocus={(event) => event.target.select()}
    />
  );
}

/** The sidebar's library section: collection, playlist tree and saved sets. */
export function PlaylistTree() {
  const view = useLibraryStore((s) => s.view);
  const setView = useLibraryStore((s) => s.setView);
  const nodes = useLibraryStore((s) => s.playlists);
  const sets = useLibraryStore((s) => s.savedSets);
  const total = useLibraryStore((s) => s.collectionTotal);
  const revision = useLibraryStore((s) => s.revision);

  const [open, setOpen] = useState<ReadonlySet<number>>(new Set());
  const [renaming, setRenaming] = useState<number | null>(null);
  const [renamingSet, setRenamingSet] = useState<number | null>(null);
  const [mark, setMark] = useState<DropMark | null>(null);
  const [pendingDelete, setPendingDelete] = useState<PlaylistNodeDto | null>(null);
  const [pendingDeleteSet, setPendingDeleteSet] = useState<SavedSetDto | null>(null);

  useEffect(() => {
    void refreshPlaylists();
    void refreshSavedSets();
  }, []);
  useEffect(() => {
    if (revision > 0) {
      void refreshSavedSets();
    }
  }, [revision]);

  const rows = flattenTree(nodes, open);

  const toggle = (id: number) =>
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(id)) {
        next.add(id);
      }
      return next;
    });

  const create = async (parent: number | null, folder: boolean) => {
    const id = await createPlaylist(parent, folder ? "New folder" : "New playlist", folder);
    if (id !== null) {
      if (parent !== null) {
        setOpen((current) => new Set(current).add(parent));
      }
      setRenaming(id);
    }
  };

  // ----- drag and drop -----

  const onDragOverNode = (event: DragEvent<HTMLElement>, node: PlaylistNodeDto) => {
    const kind = dragKind(event.dataTransfer);
    if (kind === null) {
      return;
    }
    let intent: DropIntent = "inside";
    if (kind === "playlist") {
      const rect = event.currentTarget.getBoundingClientRect();
      intent = intentAt(rect.height > 0 ? (event.clientY - rect.top) / rect.height : 0.5);
    } else if (!canDropTracks(node)) {
      // Tracks and files land in playlists only.
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    setMark({ id: node.id, intent });
  };

  const onDropNode = (event: DragEvent<HTMLElement>, node: PlaylistNodeDto) => {
    const payload = readDrag(event.dataTransfer);
    const intent = mark?.id === node.id ? mark.intent : "inside";
    setMark(null);
    if (!payload) {
      return;
    }
    event.preventDefault();
    if (payload.kind === "tracks" && canDropTracks(node)) {
      void addToPlaylist(node.id, payload.ids);
    } else if (payload.kind === "files" && canDropTracks(node)) {
      void importDropped(payload.files, node.id);
    } else if (payload.kind === "playlist") {
      const plan = planPlaylistMove(nodes, payload.id, node.id, intent);
      if (plan) {
        void movePlaylist(payload.id, plan.parent, plan.position);
        if (plan.parent !== null) {
          setOpen((current) => new Set(current).add(plan.parent as number));
        }
      }
    }
  };

  /** Dropping a playlist on the empty area moves it to the end of the top level. */
  const onDropRoot = (event: DragEvent<HTMLElement>) => {
    const payload = readDrag(event.dataTransfer);
    setMark(null);
    if (payload?.kind === "playlist") {
      event.preventDefault();
      const top = nodes.filter((n) => n.parent === null && n.id !== payload.id).length;
      void movePlaylist(payload.id, null, top);
    }
  };

  const onDropCollection = (event: DragEvent<HTMLElement>) => {
    const payload = readDrag(event.dataTransfer);
    setMark(null);
    if (payload?.kind === "files") {
      event.preventDefault();
      void importDropped(payload.files);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-2">
      <button
        type="button"
        data-testid="sidebar-collection"
        aria-current={isView(view, "collection") ? "page" : undefined}
        className={rowClass(isView(view, "collection"), mark?.id === "collection" ? "inside" : null)}
        onClick={() => setView({ kind: "collection" })}
        onDragOver={(event) => {
          if (dragKind(event.dataTransfer) === "files") {
            event.preventDefault();
            setMark({ id: "collection", intent: "inside" });
          }
        }}
        onDragLeave={() => setMark(null)}
        onDrop={onDropCollection}
      >
        <Library className="size-3.5 shrink-0" aria-hidden />
        <span className="flex-1 truncate">Collection</span>
        {total !== null ? <span className="text-xs text-fg-subtle">{total}</span> : null}
      </button>

      <section aria-label="Playlists" className="flex min-h-0 flex-col gap-0.5">
        <div className="flex items-center justify-between px-2">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-fg-subtle">Playlists</h3>
          <div className="flex items-center gap-0.5">
            <IconButton size="sm" aria-label="New playlist" onClick={() => void create(null, false)}>
              <Plus className="size-3.5" aria-hidden />
            </IconButton>
            <IconButton size="sm" aria-label="New folder" onClick={() => void create(null, true)}>
              <FolderPlus className="size-3.5" aria-hidden />
            </IconButton>
          </div>
        </div>
        <div
          role="tree"
          aria-label="Playlists"
          className="flex min-h-8 flex-col"
          onDragOver={(event) => {
            if (event.target === event.currentTarget && dragKind(event.dataTransfer) === "playlist") {
              event.preventDefault();
            }
          }}
          onDrop={(event) => event.target === event.currentTarget && onDropRoot(event)}
        >
          {rows.length === 0 ? (
            <p className="px-2 py-1 text-xs text-fg-subtle">No playlists yet.</p>
          ) : null}
          {rows.map(({ node, depth }) => {
            const isFolder = node.kind === "folder";
            const expanded = open.has(node.id);
            const active = isView(view, "playlist", node.id);
            const rowMark = mark?.id === node.id ? mark.intent : null;
            const Icon = isFolder ? (expanded ? FolderOpen : Folder) : ListMusic;
            return (
              <ContextMenu key={node.id}>
                <ContextMenuTrigger asChild>
                  <div
                    role="treeitem"
                    tabIndex={0}
                    aria-level={depth + 1}
                    aria-selected={active}
                    aria-expanded={isFolder ? expanded : undefined}
                    draggable={renaming !== node.id}
                    className={rowClass(active, rowMark)}
                    style={{ paddingLeft: `${depth * INDENT_REM + 0.5}rem` }}
                    onClick={() => (isFolder ? toggle(node.id) : setView({ kind: "playlist", id: node.id }))}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        if (isFolder) toggle(node.id);
                        else setView({ kind: "playlist", id: node.id });
                      } else if (event.key === "F2") {
                        setRenaming(node.id);
                      } else if (event.key === "ArrowRight" && isFolder && !expanded) {
                        toggle(node.id);
                      } else if (event.key === "ArrowLeft" && isFolder && expanded) {
                        toggle(node.id);
                      }
                    }}
                    onDragStart={(event) =>
                      writeDrag(event.dataTransfer, { kind: "playlist", id: node.id }, node.name)
                    }
                    onDragOver={(event) => onDragOverNode(event, node)}
                    onDragLeave={() => setMark((m) => (m?.id === node.id ? null : m))}
                    onDrop={(event) => onDropNode(event, node)}
                  >
                    {isFolder ? (
                      expanded ? (
                        <ChevronDown className="size-3 shrink-0" aria-hidden />
                      ) : (
                        <ChevronRight className="size-3 shrink-0" aria-hidden />
                      )
                    ) : null}
                    <Icon className="size-3.5 shrink-0" aria-hidden />
                    {renaming === node.id ? (
                      <RenameField
                        initial={node.name}
                        label={`Rename ${node.name}`}
                        onCommit={(name) => {
                          setRenaming(null);
                          void renamePlaylist(node.id, name);
                        }}
                        onCancel={() => setRenaming(null)}
                      />
                    ) : (
                      <span className="flex-1 truncate">{node.name}</span>
                    )}
                    {!isFolder && renaming !== node.id ? (
                      <span className="text-xs text-fg-subtle">{node.track_count}</span>
                    ) : null}
                  </div>
                </ContextMenuTrigger>
                <ContextMenuContent>
                  {isFolder ? (
                    <>
                      <ContextMenuItem onSelect={() => void create(node.id, false)}>
                        New playlist
                      </ContextMenuItem>
                      <ContextMenuItem onSelect={() => void create(node.id, true)}>
                        New folder
                      </ContextMenuItem>
                      <ContextMenuSeparator />
                    </>
                  ) : null}
                  <ContextMenuItem onSelect={() => setRenaming(node.id)}>Rename</ContextMenuItem>
                  <ContextMenuItem onSelect={() => setPendingDelete(node)}>Delete</ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
            );
          })}
        </div>
      </section>

      <section aria-label="Saved sets" className="flex flex-col gap-0.5">
        <h3 className="px-2 text-xs font-semibold uppercase tracking-wide text-fg-subtle">Sets</h3>
        {sets.length === 0 ? (
          <p className="px-2 py-1 text-xs text-fg-subtle">Tracks you load are saved to a set.</p>
        ) : null}
        {sets.map((set) => {
          const active = isView(view, "set", set.id);
          return (
            <ContextMenu key={set.id}>
              <ContextMenuTrigger asChild>
                <button
                  type="button"
                  aria-current={active ? "page" : undefined}
                  className={rowClass(active, null)}
                  onClick={() => setView({ kind: "set", id: set.id })}
                >
                  <History className="size-3.5 shrink-0" aria-hidden />
                  {renamingSet === set.id ? (
                    <RenameField
                      initial={set.name}
                      label={`Rename ${set.name}`}
                      onCommit={(name) => {
                        setRenamingSet(null);
                        void renameSavedSet(set.id, name);
                      }}
                      onCancel={() => setRenamingSet(null)}
                    />
                  ) : (
                    <span className="flex-1 truncate">{set.name}</span>
                  )}
                  <span className="text-xs text-fg-subtle">{set.track_count}</span>
                </button>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem onSelect={() => setRenamingSet(set.id)}>Rename</ContextMenuItem>
                <ContextMenuItem onSelect={() => setPendingDeleteSet(set)}>Delete</ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          );
        })}
      </section>

      <Dialog open={pendingDelete !== null} onOpenChange={(o) => !o && setPendingDelete(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {pendingDelete?.name}</DialogTitle>
            <DialogDescription>
              {pendingDelete?.kind === "folder"
                ? "The folder and everything in it are deleted. The tracks stay in the collection."
                : "The playlist is deleted. The tracks stay in the collection."}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={() => setPendingDelete(null)}>Cancel</Button>
            <Button
              variant="danger"
              onClick={() => {
                const node = pendingDelete;
                setPendingDelete(null);
                if (node) void deletePlaylist(node.id);
              }}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
      <Dialog open={pendingDeleteSet !== null} onOpenChange={(o) => !o && setPendingDeleteSet(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete {pendingDeleteSet?.name}</DialogTitle>
            <DialogDescription>The set and its track history are deleted.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button onClick={() => setPendingDeleteSet(null)}>Cancel</Button>
            <Button
              variant="danger"
              onClick={() => {
                const set = pendingDeleteSet;
                setPendingDeleteSet(null);
                if (set) void deleteSavedSet(set.id);
              }}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
