/** Value mapping for the canvas knob. Pure, so it is unit tested. */

/** Arc sweep: 270 degrees, gap at the bottom. Angles in radians, 0 = 3 o'clock, clockwise. */
export const KNOB_START = (3 * Math.PI) / 4;
export const KNOB_SWEEP = (3 * Math.PI) / 2;

/** Pixels of vertical drag for the full range. */
export const DRAG_PIXELS = 160;
/** Shift slows a drag by this factor. */
export const FINE_FACTOR = 0.2;
/** A bipolar knob snaps to its centre within this fraction of the range. */
export const CENTER_SNAP = 0.02;

export const clamp = (v: number, min: number, max: number): number => Math.min(max, Math.max(min, v));

/** Normalised position `0..1` of `value`. */
export function valueToUnit(value: number, min: number, max: number): number {
  return max > min ? clamp((value - min) / (max - min), 0, 1) : 0;
}

export function unitToValue(unit: number, min: number, max: number): number {
  return min + clamp(unit, 0, 1) * (max - min);
}

/** Pointer angle of a unit position along the arc. */
export function unitToAngle(unit: number): number {
  return KNOB_START + clamp(unit, 0, 1) * KNOB_SWEEP;
}

/** Rounds to the nearest multiple of `step` from `min`, within `[min, max]`. */
export function snapToStep(value: number, min: number, max: number, step: number): number {
  if (!(step > 0)) {
    return clamp(value, min, max);
  }
  const snapped = min + Math.round((value - min) / step) * step;
  // Trim float noise from repeated steps.
  return clamp(Number(snapped.toFixed(10)), min, max);
}

export interface DragOptions {
  /** Shift held: slower. */
  fine?: boolean;
  /** Snap to multiples of this, 0 for none. */
  step?: number;
  /** Snap onto the centre when close. */
  bipolar?: boolean;
}

/** Value after dragging `dy` pixels up (positive `dy` is up) from `start`. */
export function dragValue(
  start: number,
  dy: number,
  min: number,
  max: number,
  { fine = false, step = 0, bipolar = false }: DragOptions = {},
): number {
  const factor = fine ? FINE_FACTOR : 1;
  const raw = start + (dy / DRAG_PIXELS) * (max - min) * factor;
  return snapToStep(snapCenter(raw, min, max, bipolar), min, max, step);
}

/** Pulls a value next to the centre onto it. Only meaningful for bipolar knobs. */
export function snapCenter(value: number, min: number, max: number, bipolar: boolean): number {
  if (!bipolar) {
    return clamp(value, min, max);
  }
  const centre = (min + max) / 2;
  return Math.abs(value - centre) <= (max - min) * CENTER_SNAP
    ? centre
    : clamp(value, min, max);
}

/** Keyboard step: the configured step, at least 1% of the range. */
export function keyStep(min: number, max: number, step: number): number {
  return Math.max(step > 0 ? step : 0, (max - min) / 100);
}

/** Value after a key press, or `null` when the key does nothing. */
export function keyValue(
  key: string,
  value: number,
  min: number,
  max: number,
  step: number,
  defaultValue: number,
): number | null {
  const s = keyStep(min, max, step);
  switch (key) {
    case "ArrowUp":
    case "ArrowRight":
      return clamp(value + s, min, max);
    case "ArrowDown":
    case "ArrowLeft":
      return clamp(value - s, min, max);
    case "PageUp":
      return clamp(value + s * 10, min, max);
    case "PageDown":
      return clamp(value - s * 10, min, max);
    case "Home":
      return min;
    case "End":
      return max;
    case "Enter":
      return defaultValue;
    default:
      return null;
  }
}

/** Wheel: one notch (about 100 px of delta) moves by one key step, up for negative `deltaY`. */
export function wheelValue(
  value: number,
  deltaY: number,
  min: number,
  max: number,
  step: number,
  fine: boolean,
): number {
  const notches = -deltaY / 100;
  const s = keyStep(min, max, step) * (fine ? FINE_FACTOR : 1);
  return clamp(value + notches * s, min, max);
}
