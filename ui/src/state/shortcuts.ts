import { create } from "zustand";
import {
  resolveShortcuts,
  setActiveShortcuts,
  withBinding,
  withoutBinding,
  type Shortcut,
  type ShortcutOverrides,
} from "../lib/shortcuts";
import { PREF, loadPref, savePref } from "./prefs";

export interface ShortcutState {
  overrides: ShortcutOverrides;
  /** The bindings in effect (defaults plus overrides). */
  shortcuts: Shortcut[];
  hydrate: () => Promise<void>;
  bind: (id: string, code: string) => void;
  unbind: (id: string) => void;
  resetOne: (id: string) => void;
  resetAll: () => void;
}

const apply = (overrides: ShortcutOverrides): Pick<ShortcutState, "overrides" | "shortcuts"> => {
  const shortcuts = resolveShortcuts(overrides);
  setActiveShortcuts(shortcuts);
  return { overrides, shortcuts };
};

/** The single registry of keyboard shortcuts, persisted in the library settings. */
export const useShortcutStore = create<ShortcutState>()((set, get) => {
  const commit = (overrides: ShortcutOverrides) => {
    set(apply(overrides));
    void savePref(PREF.shortcuts, overrides);
  };
  return {
    ...apply({}),
    hydrate: async () => {
      const stored = await loadPref<ShortcutOverrides>(PREF.shortcuts, {});
      set(apply(stored && typeof stored === "object" ? stored : {}));
    },
    bind: (id, code) => commit(withBinding(get().overrides, id, code)),
    unbind: (id) => commit({ ...get().overrides, [id]: "" }),
    resetOne: (id) => {
      commit(withoutBinding(get().overrides, id));
    },
    resetAll: () => commit({}),
  };
});
