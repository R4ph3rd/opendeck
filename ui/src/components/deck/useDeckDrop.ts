import { useState } from "react";
import type { RefCallback } from "react";
import type { DragPayload } from "../../lib/dnd";
import { dropZone } from "../../lib/pointerDrag";
import { loadDroppedFile, loadTrackById } from "../../state/libraryActions";

/** Loads the first track or audio file of a drag onto `deck`; false when it holds neither. */
export function loadPayload(deck: number, payload: DragPayload | null): boolean {
  if (payload?.kind === "tracks" && payload.ids[0] !== undefined) {
    void loadTrackById(deck, payload.ids[0]);
    return true;
  }
  if (payload?.kind === "files") {
    const file = payload.files.find((f) => f.kind === "audio");
    if (file) {
      void loadDroppedFile(deck, file);
      return true;
    }
  }
  return false;
}

/** Pointer-drag drop zone that loads tracks and audio files onto `deck`; `active` is true while one hovers. */
export function useDeckDropZone(deck: number): {
  active: boolean;
  setActive: (active: boolean) => void;
  ref: RefCallback<HTMLElement>;
} {
  const [active, setActive] = useState(false);
  const ref = dropZone({
    accepts: (payload) => payload.kind === "tracks" || payload.kind === "files",
    over: (_payload, point) => setActive(point !== null),
    drop: (payload) => {
      setActive(false);
      loadPayload(deck, payload);
    },
  });
  return { active, setActive, ref };
}
