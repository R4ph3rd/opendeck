import { create } from "zustand";
import type { AudioStatus, TempoRange, TrackInfo } from "../ipc/bindings";

/** Deck count the UI shows (MVP: two). Only deck A is wired in milestone 1. */
export const DECK_A = 0;

/** How long a toast stays on screen. */
export const TOAST_MS = 6000;

/** Coarse, user-driven state of one deck. Fast-moving values live in the telemetry store. */
export interface DeckUi {
  track: TrackInfo | null;
  /** Tags of the file being decoded; shown until `track-loaded` replaces `track`. */
  incoming: TrackInfo | null;
  loading: boolean;
  tempoFader: number;
  tempoRange: TempoRange;
  keyLock: boolean;
}

export type ToastKind = "error" | "info";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

export interface AppState {
  decks: DeckUi[];
  audio: AudioStatus | null;
  settingsOpen: boolean;
  toasts: Toast[];
  patchDeck: (deck: number, patch: Partial<DeckUi>) => void;
  setAudio: (status: AudioStatus) => void;
  setSettingsOpen: (open: boolean) => void;
  pushToast: (kind: ToastKind, message: string) => number;
  dismissToast: (id: number) => void;
  reset: () => void;
}

const emptyDeck = (): DeckUi => ({
  track: null,
  incoming: null,
  loading: false,
  tempoFader: 0,
  tempoRange: "percent10",
  keyLock: false,
});

/** Stable fallback for a deck index the UI does not show. */
export const EMPTY_DECK: DeckUi = emptyDeck();

/** Selector for one deck's UI state. */
export const selectDeckUi =
  (deck: number) =>
  (state: AppState): DeckUi =>
    state.decks[deck] ?? EMPTY_DECK;

const initial = () => ({
  decks: [emptyDeck(), emptyDeck()],
  audio: null,
  settingsOpen: false,
  toasts: [],
});

let nextToast = 1;

export const useAppStore = create<AppState>()((set, get) => ({
  ...initial(),
  patchDeck: (deck, patch) =>
    set((state) => ({
      decks: state.decks.map((d, i) => (i === deck ? { ...d, ...patch } : d)),
    })),
  setAudio: (audio) => set({ audio }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  pushToast: (kind, message) => {
    const id = nextToast++;
    set((state) => ({ toasts: [...state.toasts, { id, kind, message }] }));
    setTimeout(() => get().dismissToast(id), TOAST_MS);
    return id;
  },
  dismissToast: (id) => set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  reset: () => set(initial()),
}));

/** Max speed change as a fraction for a tempo range (mirrors `TempoRange::max_fraction`). */
export const TEMPO_RANGE_FRACTION: Record<TempoRange, number> = {
  percent6: 0.06,
  percent10: 0.1,
  percent16: 0.16,
  wide: 1,
};

/** Label shown on the range selector. */
export const TEMPO_RANGE_LABEL: Record<TempoRange, string> = {
  percent6: "6%",
  percent10: "10%",
  percent16: "16%",
  wide: "100%",
};

export const TEMPO_RANGES: TempoRange[] = ["percent6", "percent10", "percent16", "wide"];
