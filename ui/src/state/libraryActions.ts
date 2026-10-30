import { open } from "@tauri-apps/plugin-dialog";
import {
  commands,
  type JobProgress,
  type TrackPageDto,
  type TrackQueryDto,
} from "../ipc/bindings";
import type { DraggedFile } from "../lib/dnd";
import { baseName } from "../lib/library";
import { errorMessage, loadFile, report } from "./actions";
import { VISIBLE_DECKS, useAppStore } from "./app";
import { isTerminal, useLibraryStore } from "./library";
import { notifyError, notifySuccess } from "./notify";

/** Fetches one collection page; `null` when the command failed (an error toast is shown). */
export function fetchTrackPage(query: TrackQueryDto): Promise<TrackPageDto | null> {
  return report(commands.queryTracks(query));
}

// ----- decks -----

/** The first deck without a track, or deck A when all are busy. */
export function firstFreeDeck(): number {
  const { decks } = useAppStore.getState();
  return VISIBLE_DECKS.find((d) => !(decks[d]?.track ?? decks[d]?.incoming)) ?? VISIBLE_DECKS[0];
}

/** Loads a library track into `deck`. The deck shows the tags at once; audio follows on `track-loaded`. */
export async function loadTrackById(deck: number, track: number): Promise<void> {
  const { patchDeck } = useAppStore.getState();
  patchDeck(deck, { loading: true });
  const info = await report(commands.loadTrack(deck, track));
  patchDeck(deck, info ? { incoming: info } : { loading: false });
}

// ----- tracks -----

export function rateTrack(track: number, stars: number): void {
  void report(commands.setRating(track, stars));
}

/** Removes tracks from the collection and tells the user how many went. */
export async function removeTracks(tracks: number[]): Promise<void> {
  const removed = await report(commands.deleteTracks(tracks));
  if (removed !== null) {
    useLibraryStore.getState().setSelection({ ids: new Set(), anchor: null, focus: null });
    notifySuccess(`Removed ${removed} ${removed === 1 ? "track" : "tracks"} from the collection.`);
  }
}

export async function reanalyzeTracks(tracks: number[]): Promise<void> {
  const job = await report(
    commands.reanalyzeTracks(tracks, { tempo: true, key: true, loudness: true, peaks: true }),
  );
  if (job !== null) {
    notifySuccess(`Analysing ${tracks.length} ${tracks.length === 1 ? "track" : "tracks"}.`);
  }
}

export function revealInExplorer(path: string): void {
  void report(commands.revealInExplorer(path));
}

// ----- playlists and sets -----

export async function refreshPlaylists(): Promise<void> {
  const nodes = await report(commands.listPlaylists());
  if (nodes) {
    useLibraryStore.getState().setPlaylists(nodes);
  }
}

export async function refreshSavedSets(): Promise<void> {
  const sets = await report(commands.listSavedSets());
  if (sets) {
    useLibraryStore.getState().setSavedSets(sets);
  }
}

export async function createPlaylist(
  parent: number | null,
  name: string,
  folder: boolean,
): Promise<number | null> {
  const id = await report(commands.createPlaylist(parent, name, folder));
  if (id !== null) {
    await refreshPlaylists();
  }
  return id;
}

export async function renamePlaylist(id: number, name: string): Promise<void> {
  if ((await report(commands.renamePlaylist(id, name))) !== null) {
    await refreshPlaylists();
  }
}

export async function deletePlaylist(id: number): Promise<void> {
  if ((await report(commands.deletePlaylist(id))) !== null) {
    await refreshPlaylists();
  }
}

export async function movePlaylist(
  id: number,
  parent: number | null,
  position: number,
): Promise<void> {
  if ((await report(commands.movePlaylist(id, parent, position))) !== null) {
    await refreshPlaylists();
  }
}

export async function addToPlaylist(playlist: number, tracks: number[]): Promise<void> {
  const done = await report(commands.addToPlaylist(playlist, tracks, null));
  if (done !== null) {
    const name =
      useLibraryStore.getState().playlists.find((p) => p.id === playlist)?.name ?? "the playlist";
    notifySuccess(`Added ${tracks.length} ${tracks.length === 1 ? "track" : "tracks"} to ${name}.`);
    await refreshPlaylists();
  }
}

export async function removeFromPlaylist(playlist: number, entries: number[]): Promise<void> {
  if ((await report(commands.removeFromPlaylist(playlist, entries))) !== null) {
    await refreshPlaylists();
  }
}

export async function renameSavedSet(id: number, name: string): Promise<void> {
  if ((await report(commands.renameSavedSet(id, name))) !== null) {
    await refreshSavedSets();
  }
}

export async function deleteSavedSet(id: number): Promise<void> {
  if ((await report(commands.deleteSavedSet(id))) !== null) {
    const { view, setView } = useLibraryStore.getState();
    if (view.kind === "set" && view.id === id) {
      setView({ kind: "collection" });
    }
    await refreshSavedSets();
  }
}

// ----- import -----

async function pickPath(options: Parameters<typeof open>[0]): Promise<string | null> {
  try {
    const picked = await open({ multiple: false, ...options });
    return typeof picked === "string" ? picked : null;
  } catch (error) {
    notifyError(errorMessage(error));
    return null;
  }
}

/** Starts a folder import into `target` (a playlist) or the collection. */
export async function importFolder(path: string, target: number | null = null): Promise<void> {
  await report(
    commands.importFolder(path, { features: ALL_FEATURES, target_playlist: target }),
  );
}

export async function pickAndImportFolder(): Promise<void> {
  const path = await pickPath({ directory: true, title: "Import folder" });
  if (path) {
    await importFolder(path);
  }
}

export async function pickAndImportPlaylist(): Promise<void> {
  const path = await pickPath({
    directory: false,
    title: "Import playlist",
    filters: [{ name: "Playlist", extensions: ["m3u", "m3u8"] }],
  });
  if (path) {
    await report(commands.importPlaylistFile(path, null));
  }
}

export async function pickAndImportRekordbox(): Promise<void> {
  const path = await pickPath({
    directory: false,
    title: "Import rekordbox XML",
    filters: [{ name: "rekordbox XML", extensions: ["xml"] }],
  });
  if (path) {
    await report(commands.importRekordboxXml(path, null));
  }
}

const ALL_FEATURES = { tempo: true, key: true, loudness: true, peaks: true };

/**
 * Imports what was dropped or chosen in the file explorer: folders as folder
 * imports, audio files as one files import, M3U files as playlist imports and
 * XML files as rekordbox imports. Audio and folders go to `target` when given.
 */
export async function importDropped(
  files: readonly DraggedFile[],
  target: number | null = null,
): Promise<void> {
  const audio = files.filter((f) => f.kind === "audio").map((f) => f.path);
  const jobs: Promise<unknown>[] = [];
  for (const dir of files.filter((f) => f.kind === "dir")) {
    jobs.push(importFolder(dir.path, target));
  }
  if (audio.length > 0) {
    jobs.push(
      report(commands.importFiles(audio, { features: ALL_FEATURES, target_playlist: target })),
    );
  }
  for (const list of files.filter((f) => f.kind === "playlist")) {
    jobs.push(
      list.ext === "xml"
        ? report(commands.importRekordboxXml(list.path, null))
        : report(commands.importPlaylistFile(list.path, null)),
    );
  }
  await Promise.all(jobs);
}

/** Loads a dragged explorer file into a deck (imports it first when it is new). */
export async function loadDroppedFile(deck: number, file: DraggedFile): Promise<void> {
  if (file.kind === "audio") {
    await loadFile(deck, file.path);
  } else {
    notifyError(`${baseName(file.path)} is not an audio file.`);
  }
}

// ----- jobs -----

export function pauseJob(job: number): void {
  void report(commands.pauseJob(job));
}

export function resumeJob(job: number): void {
  void report(commands.resumeJob(job));
}

export function cancelJob(job: number): void {
  void report(commands.cancelJob(job));
}

/** Reads stored jobs into the import panel (startup). */
export async function hydrateJobs(): Promise<void> {
  const jobs = await report(commands.listJobs());
  if (jobs) {
    useLibraryStore.getState().hydrateJobs(jobs);
  }
}

/** Routes a progress event to the job list and announces a finished job once. */
export function onImportProgress(progress: JobProgress): void {
  const store = useLibraryStore.getState();
  const before = store.jobs.find((j) => j.job === progress.job);
  store.applyProgress(progress);
  if (isTerminal(progress.state) && !(before && isTerminal(before.state))) {
    const { items_done: done, items_duplicate: dup, items_failed: failed } = progress;
    if (progress.state === "completed") {
      const parts = [`${done} added`];
      if (dup > 0) parts.push(`${dup} already in the collection`);
      if (failed > 0) parts.push(`${failed} failed`);
      notifySuccess(`Import finished: ${parts.join(", ")}.`);
    } else if (progress.state === "failed") {
      notifyError("The import failed.");
    }
  }
}
