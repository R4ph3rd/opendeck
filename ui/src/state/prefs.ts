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

/** localStorage key of the vertical split between the stacked bands. */
export const LAYOUT_KEY = "opendeck.layout.main.v2";

/** Same split with the effects band open. */
export const FX_LAYOUT_KEY = `${LAYOUT_KEY}.fx`;

/** Other split layouts that reset together with the main one. */
export const EXTRA_LAYOUT_KEYS = ["opendeck.layout.pads", "opendeck.layout.library"] as const;

/** Bands top to bottom: zoomed waveforms, decks and mixer, pads, library (percent). */
export const DEFAULT_LAYOUT: ResizableLayout = {
  waves: 12,
  decks: 44,
  pads: 15,
  library: 29,
};

/** Bands with the effects strip open between the decks and the pads. */
export const DEFAULT_FX_LAYOUT: ResizableLayout = {
  waves: 10.8,
  decks: 44,
  fx: 13.2,
  pads: 14.2,
  library: 17.8,
};

export type Theme = "dark" | "light";

export interface LayoutPreset {
  name: string;
  layout: ResizableLayout;
}

export interface LibraryOptions {
  /** Analyse tracks as they are imported. */
  autoAnalyse: boolean;
  /** BPM range for analysis. The backend stores it too and the analyzer uses it. */
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
export async function readPref<T>(name: string): Promise<{ reachable: boolean; value: T | null }> {
  try {
    const result = await commands.getUiPref(name);
    if (result.status !== "ok") {
      return { reachable: false, value: null };
    }
    return {
      reachable: true,
      value: result.data === null ? null : (JSON.parse(result.data) as T),
    };
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

/** The analyzer's BPM range as stored by the backend; empty when it cannot be asked. */
async function loadBpmRange(): Promise<Partial<LibraryOptions>> {
  try {
    const result = await commands.getBpmRange();
    if (result.status === "ok" && result.data.min_bpm !== null && result.data.max_bpm !== null) {
      return { bpmMin: result.data.min_bpm, bpmMax: result.data.max_bpm };
    }
  } catch {
    // Not running in the app: keep the stored preference.
  }
  return {};
}

/** Sends the BPM range to the analyzer. A refusal is logged; the field keeps the value. */
async function saveBpmRange(bpmMin: number, bpmMax: number): Promise<void> {
  try {
    const result = await commands.setBpmRange({
      min_bpm: bpmMin,
      max_bpm: bpmMax,
    });
    if (result.status === "error") {
      console.warn("could not set the BPM range", result.error);
    }
  } catch (error) {
    console.warn("could not set the BPM range", error);
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
    const [done, theme, zoom, layouts, library, range] = await Promise.all([
      readPref<boolean>(PREF.firstRun),
      loadPref<Theme>(PREF.theme, "dark"),
      loadPref<number>(PREF.zoom, DEFAULT_ZOOM_INDEX),
      loadPref<LayoutPreset[]>(PREF.layouts, []),
      loadPref<Partial<LibraryOptions>>(PREF.library, {}),
      loadBpmRange(),
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
      library: { ...DEFAULT_LIBRARY_OPTIONS, ...library, ...range },
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
    if (patch.bpmMin !== undefined || patch.bpmMax !== undefined) {
      void saveBpmRange(library.bpmMin, library.bpmMax);
    }
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
      for (const key of [LAYOUT_KEY, FX_LAYOUT_KEY, ...EXTRA_LAYOUT_KEYS]) {
        globalThis.localStorage?.removeItem(key);
      }
    } catch {
      // Storage may be blocked; the layout then resets on the next launch.
    }
    set((s) => ({ layoutRevision: s.layoutRevision + 1 }));
  },
}));
