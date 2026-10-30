/** Multi-select state of the track table. Rows are tracked by id, the anchor and focus by index. */
export interface SelectionState {
  ids: ReadonlySet<number>;
  /** Index the next shift-click extends from. */
  anchor: number | null;
  /** Index with the keyboard focus. */
  focus: number | null;
}

export const EMPTY_SELECTION: SelectionState = { ids: new Set(), anchor: null, focus: null };

export interface Modifiers {
  shift: boolean;
  /** Ctrl, or Cmd on macOS. */
  ctrl: boolean;
}

/** Row id at an index, `null` while the row is not loaded. */
export type IdAt = (index: number) => number | null;

/**
 * Applies a click on row `index`: plain selects only it, ctrl toggles it, shift
 * selects from the anchor to it (added to the selection with ctrl). Rows that
 * are not loaded are skipped in a range.
 */
export function clickSelect(
  state: SelectionState,
  index: number,
  mods: Modifiers,
  idAt: IdAt,
): SelectionState {
  const id = idAt(index);
  if (mods.shift && state.anchor !== null) {
    const from = Math.min(state.anchor, index);
    const to = Math.max(state.anchor, index);
    const ids = new Set(mods.ctrl ? state.ids : []);
    for (let i = from; i <= to; i++) {
      const rowId = idAt(i);
      if (rowId !== null) {
        ids.add(rowId);
      }
    }
    return { ids, anchor: state.anchor, focus: index };
  }
  if (id === null) {
    return state;
  }
  if (mods.ctrl) {
    const ids = new Set(state.ids);
    if (!ids.delete(id)) {
      ids.add(id);
    }
    return { ids, anchor: index, focus: index };
  }
  return { ids: new Set([id]), anchor: index, focus: index };
}

/** Moves the focus by `delta` rows (clamped) and selects it; shift extends from the anchor. */
export function moveFocus(
  state: SelectionState,
  delta: number,
  total: number,
  shift: boolean,
  idAt: IdAt,
): SelectionState {
  if (total <= 0) {
    return state;
  }
  const from = state.focus ?? (delta > 0 ? -1 : total);
  const to = Math.min(total - 1, Math.max(0, from + delta));
  return clickSelect(state, to, { shift, ctrl: false }, idAt);
}

/** Jumps the focus to an absolute index (Home, End). */
export function focusAt(
  state: SelectionState,
  index: number,
  total: number,
  shift: boolean,
  idAt: IdAt,
): SelectionState {
  if (total <= 0) {
    return state;
  }
  return clickSelect(state, Math.min(total - 1, Math.max(0, index)), { shift, ctrl: false }, idAt);
}

/** Ids a context menu acts on: the selection when the row is in it, else just that row. */
export function contextTargets(state: SelectionState, rowId: number): number[] {
  return state.ids.has(rowId) ? [...state.ids] : [rowId];
}
