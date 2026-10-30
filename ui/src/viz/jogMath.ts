/** Geometry and delta math for the jog wheel. Pure, so it is unit tested. */

const TAU = Math.PI * 2;

/** Platter speed shown on screen, revolutions per minute. A 33 1/3 record. */
export const PLATTER_RPM = 100 / 3;
/** Fraction of the radius where the outer ring starts. */
export const RING_START = 0.78;
/** Beats of fast search per revolution of a Shift drag. UI choice, tune with the engine owner. */
export const SEARCH_BEATS_PER_REV = 16;
/** Revolutions per arrow key press on the focused wheel. */
export const KEY_NUDGE_REV = 0.02;

export type JogZone = "top" | "ring";

/** Screen angle of the point `(dx, dy)` from the centre, radians, clockwise from 3 o'clock (y points down). */
export function angleOf(dx: number, dy: number): number {
  return Math.atan2(dy, dx);
}

/** Smallest signed difference `cur - prev`, wrapped into `(-PI, PI]`. */
export function angleDelta(prev: number, cur: number): number {
  let d = cur - prev;
  while (d > Math.PI) d -= TAU;
  while (d <= -Math.PI) d += TAU;
  return d;
}

/** Radians to revolutions. Clockwise (forward) is positive. */
export function toRevolutions(radians: number): number {
  return radians / TAU;
}

/**
 * Which part of the wheel a point hits: `top` (platter, scratch), `ring`
 * (outer edge, pitch bend), or `null` outside the wheel. `radius` is the wheel
 * radius in the same units as `dx`, `dy`.
 */
export function zoneAt(dx: number, dy: number, radius: number): JogZone | null {
  const r = Math.hypot(dx, dy);
  if (!(radius > 0) || r > radius) return null;
  return r >= radius * RING_START ? "ring" : "top";
}

/** Platter rotation for a playhead position, radians. Wraps every revolution. */
export function platterAngle(positionSeconds: number): number {
  const turns = (positionSeconds * PLATTER_RPM) / 60;
  return (turns - Math.floor(turns)) * TAU;
}

/** Beats for a Shift drag of `revolutions`. */
export function searchBeats(revolutions: number): number {
  return revolutions * SEARCH_BEATS_PER_REV;
}

/** Collects rotation between animation frames so one command carries the sum. */
export class TurnAccumulator {
  private sum = 0;
  add(revolutions: number): void {
    this.sum += revolutions;
  }
  /** Returns the collected revolutions and resets, or `0`. */
  take(): number {
    const v = this.sum;
    this.sum = 0;
    return v;
  }
}
