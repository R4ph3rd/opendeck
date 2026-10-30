import type { PlaylistNodeDto } from "../ipc/bindings";

/** MIME type of OpenDeck drag payloads inside the app. */
export const DRAG_MIME = "application/x-opendeck";

/** What an explorer entry is, for deciding which importer takes it. */
export type DraggedFileKind = "dir" | "audio" | "playlist";

export interface DraggedFile {
  path: string;
  kind: DraggedFileKind;
  /** `m3u`, `m3u8` or `xml` for playlist files. */
  ext?: string;
}

export type DragPayload =
  | { kind: "tracks"; ids: number[]; playlist?: { id: number; entries: number[] } }
  | { kind: "files"; files: DraggedFile[] }
  | { kind: "playlist"; id: number };

/** Minimal slice of `DataTransfer` the helpers use. */
export interface TransferLike {
  setData(format: string, data: string): void;
  getData(format: string): string;
  readonly types: readonly string[];
  effectAllowed?: string;
  dropEffect?: string;
}

export function writeDrag(transfer: TransferLike, payload: DragPayload, label: string): void {
  transfer.setData(DRAG_MIME, JSON.stringify(payload));
  // Browsers hide the data until the drop, but list the types while dragging.
  transfer.setData(kindType(payload.kind), "");
  transfer.setData("text/plain", label);
  transfer.effectAllowed = "copyMove";
}

const kindType = (kind: DragPayload["kind"]): string => `${DRAG_MIME}.${kind}`;

/** Kind of the OpenDeck payload being dragged, readable during dragover; `null` if not ours. */
export function dragKind(transfer: Pick<TransferLike, "types">): DragPayload["kind"] | null {
  const types = Array.from(transfer.types);
  for (const kind of ["tracks", "files", "playlist"] as const) {
    if (types.includes(kindType(kind))) {
      return kind;
    }
  }
  return null;
}

/** True while an OpenDeck payload is being dragged (contents are hidden until drop). */
export function hasDrag(transfer: Pick<TransferLike, "types">): boolean {
  return Array.from(transfer.types).includes(DRAG_MIME);
}

/** Reads the payload, `null` when the drag is not ours or the data is malformed. */
export function readDrag(transfer: Pick<TransferLike, "getData">): DragPayload | null {
  const raw = transfer.getData(DRAG_MIME);
  if (!raw) {
    return null;
  }
  try {
    const value = JSON.parse(raw) as DragPayload;
    if (value.kind === "tracks" && Array.isArray(value.ids)) {
      return value;
    }
    if (value.kind === "files" && Array.isArray(value.files)) {
      return value;
    }
    if (value.kind === "playlist" && typeof value.id === "number") {
      return value;
    }
  } catch {
    // Fall through: not a payload we wrote.
  }
  return null;
}

/** Where on a sidebar row a drop lands. */
export type DropIntent = "before" | "inside" | "after";

/** Share of a row's height that counts as its top or bottom edge. */
const EDGE_FRACTION = 0.25;

/** Intent from the pointer position inside a row (`0` top, `1` bottom). */
export function intentAt(fraction: number): DropIntent {
  if (fraction < EDGE_FRACTION) {
    return "before";
  }
  if (fraction > 1 - EDGE_FRACTION) {
    return "after";
  }
  return "inside";
}

export interface PlaylistMove {
  parent: number | null;
  position: number;
}

const bySiblingOrder = (a: PlaylistNodeDto, b: PlaylistNodeDto) => a.position - b.position;

/** True when `id` is `ancestor` or sits somewhere below it. */
export function isWithin(nodes: readonly PlaylistNodeDto[], id: number, ancestor: number): boolean {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  let current: number | null = id;
  while (current !== null) {
    if (current === ancestor) {
      return true;
    }
    current = byId.get(current)?.parent ?? null;
  }
  return false;
}

/**
 * Where a dragged playlist or folder goes when dropped on `target`, or `null`
 * when the drop is not allowed (onto itself, into its own subtree). Dropping
 * inside a playlist (not a folder) places it after that playlist instead,
 * because playlists only nest in folders. `position` is the sibling index in
 * the list without the dragged node, as `move_playlist` expects.
 */
export function planPlaylistMove(
  nodes: readonly PlaylistNodeDto[],
  draggedId: number,
  targetId: number,
  intent: DropIntent,
): PlaylistMove | null {
  const target = nodes.find((n) => n.id === targetId);
  if (!target || draggedId === targetId || isWithin(nodes, targetId, draggedId)) {
    return null;
  }
  const siblingsOf = (parent: number | null) =>
    nodes
      .filter((n) => n.parent === parent && n.id !== draggedId)
      .sort(bySiblingOrder)
      .map((n) => n.id);
  if (intent === "inside" && target.kind === "folder") {
    return { parent: target.id, position: siblingsOf(target.id).length };
  }
  const siblings = siblingsOf(target.parent);
  const at = siblings.indexOf(target.id);
  return {
    parent: target.parent,
    position: intent === "before" ? at : at + 1,
  };
}

/** What dropping tracks on a sidebar node does. */
export function canDropTracks(node: PlaylistNodeDto): boolean {
  return node.kind === "playlist";
}

/** Rows of the sidebar tree in display order, honouring which folders are open. */
export interface TreeRow {
  node: PlaylistNodeDto;
  depth: number;
}

export function flattenTree(
  nodes: readonly PlaylistNodeDto[],
  open: ReadonlySet<number>,
): TreeRow[] {
  const children = new Map<number | null, PlaylistNodeDto[]>();
  for (const node of nodes) {
    const list = children.get(node.parent) ?? [];
    list.push(node);
    children.set(node.parent, list);
  }
  const rows: TreeRow[] = [];
  const walk = (parent: number | null, depth: number) => {
    for (const node of (children.get(parent) ?? []).sort(bySiblingOrder)) {
      rows.push({ node, depth });
      if (node.kind === "folder" && open.has(node.id)) {
        walk(node.id, depth + 1);
      }
    }
  };
  walk(null, 0);
  return rows;
}
