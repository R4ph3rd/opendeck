import { create } from "zustand";
import type { AudioStatus, Cue, DeckState, SyncMode, TempoRange, TrackInfo } from "../ipc/bindings";

/** Deck indices the UI shows (MVP: two). */
export const DECK_A = 0;
export const DECK_B = 1;
export const VISIBLE_DECKS = [DECK_A, DECK_B] as const;

/** Hot cue slots per deck (`contracts::HOT_CUES_PER_DECK`). */
export const HOT_CUE_COUNT = 8;

/** Beat counts offered for auto loops and loop roll. */
export const LOOP_BEATS = [0.25, 0.5, 1, 2, 4, 8, 16, 32] as const;

/** Beat jump sizes (a subset of `Beats::STANDARD_SIZES`). */
export const JUMP_BEATS = [0.25, 0.5, 1, 2, 4, 8, 16, 32] as const;

/** Default beat jump size. */
export const DEFAULT_JUMP_BEATS = 4;

/** Coarse, user-driven state of one deck. Fast-moving values live in the telemetry store. */
export interface DeckUi {
  track: TrackInfo | null;
  /** Tags of the file being decoded; shown until `track-loaded` replaces `track`. */
  incoming: TrackInfo | null;
  loading: boolean;
  tempoFader: number;
  tempoRange: TempoRange;
  keyLock: boolean;
  /** Sync mode in effect; `"off"` when sync is disengaged. */
  sync: SyncMode;
  /** Mode SYNC turns on with (the last one chosen). */
  syncPref: Exclude<SyncMode, "off">;
  quantize: boolean;
  slip: boolean;
  /** Loop size buttons roll while held instead of starting an auto loop. */
  rollMode: boolean;
  /** Selected beat jump size in beats. */
  jumpBeats: number;
  /** The eight hot cue slots, `null` when empty. */
  hotCues: (Cue | null)[];
}

export interface AppState {
  decks: DeckUi[];
  /** Sync leader as last requested or reported by the backend. */
  leader: number | null;
  audio: AudioStatus | null;
  settingsOpen: boolean;
  patchDeck: (deck: number, patch: Partial<DeckUi>) => void;
  /** Applies a backend `DeckState` (startup hydration). */
  applyDeckState: (state: DeckState) => void;
  setLeader: (deck: number | null) => void;
  setAudio: (status: AudioStatus) => void;
  setSettingsOpen: (open: boolean) => void;
  reset: () => void;
}

const emptyHotCues = (): (Cue | null)[] => Array.from({ length: HOT_CUE_COUNT }, () => null);

const emptyDeck = (): DeckUi => ({
  track: null,
  incoming: null,
  loading: false,
  tempoFader: 0,
  tempoRange: "percent10",
  keyLock: false,
  sync: "off",
  syncPref: "beat",
  quantize: true,
  slip: false,
  rollMode: false,
  jumpBeats: DEFAULT_JUMP_BEATS,
  hotCues: emptyHotCues(),
});

/** Stable fallback for a deck index the UI does not show. */
export const EMPTY_DECK: DeckUi = emptyDeck();

/** Selector for one deck's UI state. */
export const selectDeckUi =
  (deck: number) =>
  (state: AppState): DeckUi =>
    state.decks[deck] ?? EMPTY_DECK;

const initial = () => ({
  decks: VISIBLE_DECKS.map(emptyDeck),
  leader: null,
  audio: null,
  settingsOpen: false,
});

export const useAppStore = create<AppState>()((set) => ({
  ...initial(),
  patchDeck: (deck, patch) =>
    set((state) => ({
      decks: state.decks.map((d, i) => (i === deck ? { ...d, ...patch } : d)),
    })),
  applyDeckState: (deckState) =>
    set((state) => {
      const deck = deckState.deck;
      const current = state.decks[deck];
      if (!current) {
        return state;
      }
      const patch: Partial<DeckUi> = {
        tempoFader: deckState.tempo_fader ?? 0,
        tempoRange: deckState.tempo_range,
        keyLock: deckState.key_lock,
        sync: deckState.sync,
        syncPref: deckState.sync === "off" ? current.syncPref : deckState.sync,
        quantize: deckState.quantize,
        slip: deckState.slip,
      };
      return {
        decks: state.decks.map((d, i) => (i === deck ? { ...d, ...patch } : d)),
        leader: deckState.is_leader ? deck : state.leader === deck ? null : state.leader,
      };
    }),
  setLeader: (leader) => set({ leader }),
  setAudio: (audio) => set({ audio }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
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
