import { create } from "zustand";
import type {
  JobInfo,
  JobProgress,
  JobState,
  PlaylistNodeDto,
  SavedSetDto,
} from "../ipc/bindings";
import {
  DEFAULT_SORT,
  NO_FILTERS,
  type KeyNotation,
  type TrackFilters,
  type TrackSort,
} from "../lib/library";
import { readStored, writeStored } from "../lib/persist";
import { EMPTY_SELECTION, type SelectionState } from "../lib/selection";

/** What the main library pane shows. */
export type LibraryView =
  | { kind: "collection" }
  | { kind: "playlist"; id: number }
  | { kind: "set"; id: number };

/** A job in the import panel: live progress plus what the stored row knows. */
export interface JobEntry extends JobProgress {
  source: string | null;
  error: string | null;
}

/** True once a job state never changes again. */
export function isTerminal(state: JobState): boolean {
  return state === "completed" || state === "cancelled" || state === "failed";
}

/** Preferences kept across launches. */
export interface LibraryPrefs {
  keyNotation: KeyNotation;
  columnVisibility: Record<string, boolean>;
  columnSizing: Record<string, number>;
  sort: TrackSort;
}

export const PREFS_KEY = "opendeck.library.v1";

const DEFAULT_PREFS: LibraryPrefs = {
  keyNotation: "camelot",
  columnVisibility: { album: false, bitrate: false, year: false, comment: false, analysis: false },
  columnSizing: {},
  sort: DEFAULT_SORT,
};

function loadPrefs(): LibraryPrefs {
  return { ...DEFAULT_PREFS, ...readStored<Partial<LibraryPrefs>>(PREFS_KEY, {}) };
}

export interface LibraryState extends LibraryPrefs {
  view: LibraryView;
  filters: TrackFilters;
  selection: SelectionState;
  playlists: PlaylistNodeDto[];
  savedSets: SavedSetDto[];
  /** Jobs shown in the import panel, newest first. */
  jobs: JobEntry[];
  /** Bumped on every `library-changed`; views refetch when it moves. */
  revision: number;
  /** Rows in the collection, when known (sidebar count, empty state). */
  collectionTotal: number | null;
  setView: (view: LibraryView) => void;
  setFilters: (patch: Partial<TrackFilters>) => void;
  clearFilters: () => void;
  setSort: (sort: TrackSort) => void;
  setKeyNotation: (notation: KeyNotation) => void;
  setColumnVisibility: (visibility: Record<string, boolean>) => void;
  setColumnSizing: (sizing: Record<string, number>) => void;
  setSelection: (selection: SelectionState) => void;
  setPlaylists: (nodes: PlaylistNodeDto[]) => void;
  setSavedSets: (sets: SavedSetDto[]) => void;
  setCollectionTotal: (total: number | null) => void;
  bumpRevision: () => void;
  /** Merges a progress event into the job list. */
  applyProgress: (progress: JobProgress) => void;
  /** Replaces the job list with stored jobs (startup). */
  hydrateJobs: (jobs: JobInfo[]) => void;
  dismissJob: (job: number) => void;
  dismissFinished: () => void;
  reset: () => void;
}

const initial = () => ({
  ...loadPrefs(),
  view: { kind: "collection" } as LibraryView,
  filters: NO_FILTERS,
  selection: EMPTY_SELECTION,
  playlists: [] as PlaylistNodeDto[],
  savedSets: [] as SavedSetDto[],
  jobs: [] as JobEntry[],
  revision: 0,
  collectionTotal: null as number | null,
});

function saved(state: LibraryState): LibraryPrefs {
  return {
    keyNotation: state.keyNotation,
    columnVisibility: state.columnVisibility,
    columnSizing: state.columnSizing,
    sort: state.sort,
  };
}

export const useLibraryStore = create<LibraryState>()((set, get) => {
  /** Applies a change to a preference and stores the preferences. */
  const savePrefs = (patch: Partial<LibraryPrefs>) => {
    set(patch);
    writeStored(PREFS_KEY, saved(get()));
  };
  return {
    ...initial(),
    setView: (view) => set({ view, selection: EMPTY_SELECTION }),
    setFilters: (patch) => set((s) => ({ filters: { ...s.filters, ...patch } })),
    clearFilters: () => set({ filters: NO_FILTERS }),
    setSort: (sort) => savePrefs({ sort }),
    setKeyNotation: (keyNotation) => savePrefs({ keyNotation }),
    setColumnVisibility: (columnVisibility) => savePrefs({ columnVisibility }),
    setColumnSizing: (columnSizing) => savePrefs({ columnSizing }),
    setSelection: (selection) => set({ selection }),
    setPlaylists: (playlists) =>
      set((s) => {
        // A view of a playlist that was deleted falls back to the collection.
        const { view } = s;
        const gone = view.kind === "playlist" && !playlists.some((p) => p.id === view.id);
        return gone ? { playlists, view: { kind: "collection" } } : { playlists };
      }),
    setSavedSets: (savedSets) => set({ savedSets }),
    setCollectionTotal: (collectionTotal) => set({ collectionTotal }),
    bumpRevision: () => set((s) => ({ revision: s.revision + 1 })),
    applyProgress: (progress) =>
      set((s) => {
        const known = s.jobs.find((j) => j.job === progress.job);
        const entry: JobEntry = {
          ...progress,
          source: known?.source ?? null,
          error: known?.error ?? null,
          // Keep the last file name once the job stops reporting one.
          current_item: progress.current_item ?? known?.current_item ?? null,
        };
        return {
          jobs: known
            ? s.jobs.map((j) => (j.job === progress.job ? entry : j))
            : [entry, ...s.jobs],
        };
      }),
    hydrateJobs: (stored) =>
      set((s) => {
        const live = new Map(s.jobs.map((j) => [j.job, j]));
        const entries = stored
          // Finished jobs from earlier sessions are history, not news.
          .filter((j) => !isTerminal(j.progress.state) || live.has(j.progress.job))
          .map((j): JobEntry => {
            const known = live.get(j.progress.job);
            return known ?? { ...j.progress, source: j.source, error: j.error };
          });
        const ids = new Set(entries.map((e) => e.job));
        return { jobs: [...entries, ...s.jobs.filter((j) => !ids.has(j.job))] };
      }),
    dismissJob: (job) => set((s) => ({ jobs: s.jobs.filter((j) => j.job !== job) })),
    dismissFinished: () =>
      set((s) => ({
        jobs: s.jobs.filter((j) => !isTerminal(j.state)),
      })),
    reset: () => set(initial()),
  };
});

/** Playlists (not folders), for "add to playlist" menus. */
export function playlistsOnly(nodes: readonly PlaylistNodeDto[]): PlaylistNodeDto[] {
  return nodes.filter((n) => n.kind === "playlist");
}
