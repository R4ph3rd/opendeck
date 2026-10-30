import { Panel, PanelHeader } from "@opendeck/design-system";
import type { DragEvent } from "react";
import { dragKind, readDrag } from "../lib/dnd";
import { importDropped } from "../state/libraryActions";
import { useLibraryStore } from "../state/library";
import { CollectionView } from "./CollectionView";
import { ImportPanel } from "./ImportPanel";
import { PlaylistView } from "./PlaylistView";
import { SavedSetView } from "./SavedSetView";
import { Sidebar } from "./Sidebar";

/**
 * The bottom half of the window: sidebar on the left, the selected view on the
 * right, the import panel under it. Files and folders dragged from the Files
 * tab onto the view are imported (into the playlist when one is open).
 */
export function LibraryPane() {
  const view = useLibraryStore((s) => s.view);
  const playlists = useLibraryStore((s) => s.playlists);
  const sets = useLibraryStore((s) => s.savedSets);
  const total = useLibraryStore((s) => s.collectionTotal);

  let title = "Collection";
  let count: number | null = total;
  if (view.kind === "playlist") {
    const node = playlists.find((p) => p.id === view.id);
    title = node?.name ?? "Playlist";
    count = node?.track_count ?? null;
  } else if (view.kind === "set") {
    const set = sets.find((s) => s.id === view.id);
    title = set?.name ?? "Set";
    count = set?.track_count ?? null;
  }

  const onDragOver = (event: DragEvent) => {
    if (dragKind(event.dataTransfer) === "files") {
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
    }
  };
  const onDrop = (event: DragEvent) => {
    const payload = readDrag(event.dataTransfer);
    if (payload?.kind === "files") {
      event.preventDefault();
      void importDropped(payload.files, view.kind === "playlist" ? view.id : null);
    }
  };

  return (
    <Panel aria-label="Library" className="min-h-40 flex-1 flex-row">
      <Sidebar />
      <div
        className="flex min-h-0 min-w-0 flex-1 flex-col"
        onDragOver={onDragOver}
        onDrop={onDrop}
      >
        <PanelHeader
          title={title}
          actions={
            count === null ? null : (
              <span className="text-xs text-fg-subtle">
                {count} {count === 1 ? "track" : "tracks"}
              </span>
            )
          }
        />
        {view.kind === "collection" ? <CollectionView /> : null}
        {view.kind === "playlist" ? <PlaylistView id={view.id} name={title} /> : null}
        {view.kind === "set" ? <SavedSetView id={view.id} /> : null}
        <ImportPanel />
      </div>
    </Panel>
  );
}
