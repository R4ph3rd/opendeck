import type { PointerEvent as ReactPointerEvent, RefCallback } from "react";
import type { DragPayload } from "./dnd";

/**
 * Pointer-based drag and drop for tracks and explorer files.
 *
 * HTML5 drag and drop is unreliable in the Tauri webviews: WebKitGTK starts
 * the drag but never delivers the drop to custom MIME types, and WebView2
 * swallows it when the window has file drop enabled. Pointer events behave the
 * same everywhere, so the sources start a drag with `startPointerDrag` and the
 * targets register with `dropZone`. Playlist reordering still uses HTML5.
 */

/** Pointer travel in px before a press turns into a drag, so plain clicks stay clicks. */
export const DRAG_THRESHOLD_PX = 5;

/** Ghost offset from the pointer in px, so it does not sit under the cursor. */
const GHOST_OFFSET_PX = 12;

/** Where the pointer is, in viewport coordinates. */
export interface DragPoint {
  x: number;
  y: number;
}

export interface DropHandlers {
  /** Whether this zone takes the payload. */
  accepts(payload: DragPayload): boolean;
  /** The pointer entered, moved within or (`point === null`) left the zone. */
  over?(payload: DragPayload, point: DragPoint | null): void;
  drop(payload: DragPayload, point: DragPoint): void;
}

// Weak, so an element a wrapper never unregisters does not leak.
const zones = new WeakMap<Element, DropHandlers>();

/** Ref callback that registers an element as a drop zone for pointer drags. */
export function dropZone(handlers: DropHandlers): RefCallback<HTMLElement> {
  return (element) => {
    if (!element) {
      return undefined;
    }
    zones.set(element, handlers);
    return () => {
      if (zones.get(element) === handlers) {
        zones.delete(element);
      }
    };
  };
}

/** The innermost registered zone under `point` that accepts the payload. */
function zoneAt(point: DragPoint, payload: DragPayload): { element: Element; handlers: DropHandlers } | null {
  let element: Element | null = document.elementFromPoint(point.x, point.y);
  while (element) {
    const handlers = zones.get(element);
    if (handlers?.accepts(payload)) {
      return { element, handlers };
    }
    element = element.parentElement;
  }
  return null;
}

/** True while a pointer drag is in progress. */
export function isPointerDragging(): boolean {
  return active !== null;
}

interface ActiveDrag {
  payload: DragPayload;
  hover: { element: Element; handlers: DropHandlers } | null;
}

let active: ActiveDrag | null = null;

export interface DragSpec {
  payload: DragPayload;
  label: string;
}

/**
 * Call from `onPointerDown` of a draggable element. `make` runs once the
 * pointer has moved past the threshold, so it can read the current selection.
 * Returns nothing; the drag cleans up after itself on release or Escape.
 */
export function startPointerDrag(event: Pick<ReactPointerEvent, "button" | "clientX" | "clientY">, make: () => DragSpec | null): void {
  if (event.button !== 0 || active) {
    return;
  }
  const origin: DragPoint = { x: event.clientX, y: event.clientY };
  let spec: DragSpec | null = null;
  let ghost: HTMLElement | null = null;

  const point = (e: PointerEvent): DragPoint => ({ x: e.clientX, y: e.clientY });

  const place = (p: DragPoint) => {
    if (ghost) {
      ghost.style.transform = `translate(${p.x + GHOST_OFFSET_PX}px, ${p.y + GHOST_OFFSET_PX}px)`;
    }
  };

  const setHover = (next: ActiveDrag["hover"], p: DragPoint) => {
    if (!active || !spec) {
      return;
    }
    const previous = active.hover;
    if (previous && previous.element !== next?.element) {
      previous.handlers.over?.(spec.payload, null);
    }
    active.hover = next;
    next?.handlers.over?.(spec.payload, p);
  };

  const finish = (drop: boolean, p: DragPoint) => {
    window.removeEventListener("pointermove", onMove, true);
    window.removeEventListener("pointerup", onUp, true);
    window.removeEventListener("pointercancel", onCancel, true);
    window.removeEventListener("keydown", onKey, true);
    if (active && spec) {
      const target = drop ? zoneAt(p, spec.payload) : null;
      setHover(null, p);
      ghost?.remove();
      document.body.classList.remove("select-none", "cursor-grabbing");
      active = null;
      // The release must not count as a click on the row it ended on.
      const swallow = (e: Event) => e.stopPropagation();
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, true), 0);
      target?.handlers.drop(spec.payload, p);
    }
  };

  const onMove = (e: PointerEvent) => {
    const p = point(e);
    if (!spec) {
      if (Math.hypot(p.x - origin.x, p.y - origin.y) < DRAG_THRESHOLD_PX) {
        return;
      }
      spec = make();
      if (!spec) {
        finish(false, p);
        return;
      }
      ghost = document.createElement("div");
      ghost.textContent = spec.label;
      ghost.className =
        "pointer-events-none fixed left-0 top-0 z-50 max-w-64 truncate rounded-sm border border-border bg-surface-raised px-2 py-1 text-xs text-fg shadow-lg";
      ghost.setAttribute("aria-hidden", "true");
      document.body.append(ghost);
      document.body.classList.add("select-none", "cursor-grabbing");
      active = { payload: spec.payload, hover: null };
    }
    place(p);
    setHover(zoneAt(p, spec.payload), p);
  };
  const onUp = (e: PointerEvent) => finish(spec !== null, point(e));
  const onCancel = (e: PointerEvent) => finish(false, point(e));
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      finish(false, origin);
    }
  };

  window.addEventListener("pointermove", onMove, true);
  window.addEventListener("pointerup", onUp, true);
  window.addEventListener("pointercancel", onCancel, true);
  window.addEventListener("keydown", onKey, true);
}
