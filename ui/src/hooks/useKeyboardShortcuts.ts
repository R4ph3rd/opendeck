import { useEffect } from "react";
import { cuePress, cueRelease, releaseAllCues, togglePlay } from "../state/actions";
import { DECK_A } from "../state/app";

/** Elements that keep Space and arrows for themselves (a focused button activates on Space). */
const OWN_KEYS =
  "button, a[href], input, textarea, select, [role=slider], [role=combobox], [role=listbox], [role=dialog], [contenteditable=true]";

function ownsKeys(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(OWN_KEYS) !== null;
}

/**
 * Global shortcuts (docs/shortcuts.md): Space plays or pauses deck A, F1 is
 * CUE on deck A (held = preview). Releases CUE when the window loses focus.
 */
export function useKeyboardShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) {
        return;
      }
      if (event.code === "F1") {
        event.preventDefault();
        if (!event.repeat) cuePress(DECK_A);
      } else if (event.code === "Space" && !ownsKeys(event.target)) {
        // Also stops a focused button from activating on top of this.
        event.preventDefault();
        if (!event.repeat) togglePlay(DECK_A);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.code === "F1") {
        event.preventDefault();
        cueRelease(DECK_A);
      } else if (event.code === "Space" && !ownsKeys(event.target)) {
        event.preventDefault();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", releaseAllCues);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", releaseAllCues);
      releaseAllCues();
    };
  }, []);
}
