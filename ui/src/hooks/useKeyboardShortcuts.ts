import { useEffect } from "react";
import { findShortcut, type Shortcut } from "../lib/shortcuts";
import {
  beatJump,
  cuePress,
  cueRelease,
  hotCuePress,
  hotCueRelease,
  loopIn,
  loopOut,
  loopToggle,
  releaseAllHeld,
  toggleQuantize,
  toggleSlip,
  toggleSync,
  togglePlay,
} from "../state/actions";

/** Elements that keep Space and Enter for themselves (a focused button activates on both). */
const OWN_ACTIVATION_KEYS =
  "button, a[href], input, textarea, select, [role=slider], [role=combobox], [role=listbox], [role=dialog], [contenteditable=true]";

/** Elements that take typed characters. Range inputs and buttons do not. */
const OWN_TEXT_KEYS =
  "input:not([type=range]), textarea, select, [role=combobox], [role=listbox], [role=dialog], [contenteditable=true]";

const closest = (target: EventTarget | null, selector: string): boolean =>
  target instanceof Element && target.closest(selector) !== null;

/** Space and Enter belong to the focused control; other keys only to text entry. */
function ownedByTarget(shortcut: Shortcut, target: EventTarget | null): boolean {
  const activation = shortcut.code === "Space" || shortcut.code === "Enter";
  return closest(target, activation ? OWN_ACTIVATION_KEYS : OWN_TEXT_KEYS);
}

function press(shortcut: Shortcut, shift: boolean): void {
  const { deck, action } = shortcut;
  switch (action.kind) {
    case "play":
      togglePlay(deck);
      break;
    case "cue":
      cuePress(deck);
      break;
    case "sync":
      toggleSync(deck);
      break;
    case "quantize":
      toggleQuantize(deck);
      break;
    case "slip":
      toggleSlip(deck);
      break;
    case "loopIn":
      loopIn(deck);
      break;
    case "loopOut":
      loopOut(deck);
      break;
    case "reloop":
      loopToggle(deck);
      break;
    case "jumpBack":
      beatJump(deck, -1);
      break;
    case "jumpForward":
      beatJump(deck, 1);
      break;
    case "hotCue":
      hotCuePress(deck, action.index, shift);
      break;
  }
}

function release(shortcut: Shortcut): void {
  if (shortcut.action.kind === "cue") {
    cueRelease(shortcut.deck);
  } else if (shortcut.action.kind === "hotCue") {
    hotCueRelease(shortcut.deck, shortcut.action.index);
  }
}

/**
 * Global shortcuts for both decks (docs/shortcuts.md). Cue and hot cue keys
 * hold while down. Everything held is released when the window loses focus.
 */
export function useKeyboardShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) {
        return;
      }
      const shortcut = findShortcut(event.code);
      if (!shortcut) {
        return;
      }
      // F1 and F2 are never typed characters, so they work from anywhere.
      const fkey = shortcut.action.kind === "cue";
      if (!fkey && ownedByTarget(shortcut, event.target)) {
        return;
      }
      // Also stops a focused button from activating on top of the shortcut.
      event.preventDefault();
      if (!event.repeat) {
        press(shortcut, event.shiftKey);
      }
    };
    const onKeyUp = (event: KeyboardEvent) => {
      const shortcut = findShortcut(event.code);
      if (!shortcut) {
        return;
      }
      const fkey = shortcut.action.kind === "cue";
      if (!fkey && ownedByTarget(shortcut, event.target)) {
        return;
      }
      event.preventDefault();
      release(shortcut);
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", releaseAllHeld);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", releaseAllHeld);
      releaseAllHeld();
    };
  }, []);
}
