import type { ResizableLayout } from "@opendeck/design-system";
import { create } from "zustand";
import { commands } from "../ipc/bindings";
import { readStored, writeStored } from "../lib/persist";
import { clampZoom, DEFAULT_ZOOM_INDEX } from "../viz/waveformMath";

/** Names of the preferences stored in the library settings (`ui.<name>`). */
export const PREF = {
  firstRun: "firstRunDone",
  theme: "theme",
  zoom: "waveformZoom",
  layouts: "layoutPresets",
  shortcuts: "shortcuts",
  library: "libraryOptions",
} as const;

/** localStorage key of the vertical split between stage, pads and collection. */
export const LAYOUT_KEY = "opendeck.layout.main";

/** The two-deck layout the app starts with. */
export const DEFAULT_LAYOUT: ResizableLayout = { stage: 45, pads: 25, library: 30 };

export type Theme = "dark" | "light";

export interface LayoutPreset {
  name: string;
  layout: ResizableLayout;
}

export interface LibraryOptions {
  /** Analyse tracks as they are imported. */
  autoAnalyse: boolean;
  /** BPM range for analysis, kept for when the analyzer takes it. */
  bpmMin: number;
  bpmMax: number;
  /** Folders the user added as music sources. */
  folders: string[];
}

export const DEFAULT_LIBRARY_OPTIONS: LibraryOptions = {
  autoAnalyse: true,
  bpmMin: 70,
  bpmMax: 180,
  folders: [],
};

/** Reads a preference. `reachable` is false when the backend could not be asked. */
export async function readPref<T>(
  name: string,
): Promise<{ reachable: boolean; value: T | null }> {
  try {
    const result = await commands.getUiPref(name);
    if (result.status !== "ok") {
      return { reachable: false, value: null };
    }
    return { reachable: true, value: result.data === null ? null : (JSON.parse(result.data) as T) };
  } catch {
    return { reachable: false, value: null };
  }
}

/** Reads a preference; any failure (no backend, bad JSON) gives `fallback`. */
export async function loadPref<T>(name: string, fallback: T): Promise<T> {
  try {
    const result = await commands.getUiPref(name);
    if (result.status === "ok" && result.data !== null) {
      return JSON.parse(result.data) as T;
    }
  } catch {
    // Not running in the app, or the value is corrupt: use the default.
  }
  return fallback;
}

/** Stores a preference. A failure is logged and the value lasts for the session. */
export async function savePref(name: string, value: unknown): Promise<void> {
  try {
    const result = await commands.setUiPref(name, JSON.stringify(value));
    if (result.status === "error") {
      console.warn("could not save preference", name, result.error);
    }
  } catch (error) {
    console.warn("could not save preference", name, error);
  }
}

export interface PrefsState {
  loaded: boolean;
  firstRunDone: boolean;
  firstRunOpen: boolean;
  theme: Theme;
  /** Default zoom index of the deck waveforms. */
  waveformZoom: number;
  layoutPresets: LayoutPreset[];
  /** Bumped when a layout is restored; the panel group remounts on it. */
  layoutRevision: number;
  library: LibraryOptions;
  hydrate: () => Promise<void>;
  setTheme: (theme: Theme) => void;
  setWaveformZoom: (index: number) => void;
  setLibrary: (patch: Partial<LibraryOptions>) => void;
  saveLayoutPreset: (name: string) => void;
  restoreLayout: (layout: ResizableLayout) => void;
  deleteLayoutPreset: (name: string) => void;
  openFirstRun: () => void;
  closeFirstRun: () => void;
  completeFirstRun: () => void;
  /** Back to defaults after a settings reset. */
  resetAll: () => void;
}

const initial = () => ({
  loaded: false,
  firstRunDone: true,
  firstRunOpen: false,
  theme: "dark" as Theme,
  waveformZoom: DEFAULT_ZOOM_INDEX,
  layoutPresets: [] as LayoutPreset[],
  layoutRevision: 0,
  library: DEFAULT_LIBRARY_OPTIONS,
});

/** Applies the theme to the document so every design-system token follows. */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

export const usePrefsStore = create<PrefsState>()((set, get) => ({
  ...initial(),
  hydrate: async () => {
    const [done, theme, zoom, layouts, library] = await Promise.all([
      readPref<boolean>(PREF.firstRun),
      loadPref<Theme>(PREF.theme, "dark"),
      loadPref<number>(PREF.zoom, DEFAULT_ZOOM_INDEX),
      loadPref<LayoutPreset[]>(PREF.layouts, []),
      loadPref<Partial<LibraryOptions>>(PREF.library, {}),
    ]);
    // Without a backend (previews, tests) nothing is known, so no guide is shown.
    const firstRunDone = !done.reachable || done.value === true;
    set({
      loaded: true,
      firstRunDone,
      firstRunOpen: !firstRunDone,
      theme: theme === "light" ? "light" : "dark",
      waveformZoom: clampZoom(Number.isFinite(zoom) ? zoom : DEFAULT_ZOOM_INDEX),
      layoutPresets: Array.isArray(layouts) ? layouts : [],
      library: { ...DEFAULT_LIBRARY_OPTIONS, ...library },
    });
  },
  setTheme: (theme) => {
    set({ theme });
    void savePref(PREF.theme, theme);
  },
  setWaveformZoom: (index) => {
    const waveformZoom = clampZoom(index);
    set({ waveformZoom });
    void savePref(PREF.zoom, waveformZoom);
  },
  setLibrary: (patch) => {
    const library = { ...get().library, ...patch };
    set({ library });
    void savePref(PREF.library, library);
  },
  saveLayoutPreset: (name) => {
    const trimmed = name.trim();
    if (!trimmed) {
      return;
    }
    const layout = readStored<ResizableLayout>(LAYOUT_KEY, DEFAULT_LAYOUT);
    const layoutPresets = [
      ...get().layoutPresets.filter((p) => p.name !== trimmed),
      { name: trimmed, layout },
    ];
    set({ layoutPresets });
    void savePref(PREF.layouts, layoutPresets);
  },
  restoreLayout: (layout) => {
    writeStored(LAYOUT_KEY, layout);
    set((s) => ({ layoutRevision: s.layoutRevision + 1 }));
  },
  deleteLayoutPreset: (name) => {
    const layoutPresets = get().layoutPresets.filter((p) => p.name !== name);
    set({ layoutPresets });
    void savePref(PREF.layouts, layoutPresets);
  },
  openFirstRun: () => set({ firstRunOpen: true }),
  closeFirstRun: () => set({ firstRunOpen: false }),
  completeFirstRun: () => {
    set({ firstRunDone: true, firstRunOpen: false });
    void savePref(PREF.firstRun, true);
  },
  resetAll: () => {
    set({ ...initial(), loaded: true, firstRunDone: true });
    try {
      globalThis.localStorage?.removeItem(LAYOUT_KEY);
    } catch {
      // Storage may be blocked; the layout then resets on the next launch.
    }
    set((s) => ({ layoutRevision: s.layoutRevision + 1 }));
  },
}));
