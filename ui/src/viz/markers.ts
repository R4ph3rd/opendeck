import type { BeatGrid, Cue } from "../ipc/bindings";
import { cueToken, type TokenName, type VizColors } from "./colors";

/** Static, per-track overlay data for the waveforms. Changes rarely, so it lives in React state. */
export interface DeckMarkers {
  grid: BeatGrid | null;
  /** The eight hot cue slots, `null` when empty. Loops use `end`. */
  hotCues: readonly (Cue | null)[];
  /** Memory cues and saved loops that are not hot cues. */
  memoryCues: readonly Cue[];
}

export const NO_MARKERS: DeckMarkers = { grid: null, hotCues: [], memoryCues: [] };

export interface CueMark {
  time: number;
  /** End of a loop cue, seconds. */
  end: number | null;
  token: TokenName;
  /** Flag text: pad number for hot cues, empty for memory cues. */
  label: string;
  hot: boolean;
}

/** Flat list of drawable cue marks, hot cues first. */
export function cueMarks(markers: DeckMarkers): CueMark[] {
  const out: CueMark[] = [];
  markers.hotCues.forEach((cue, index) => {
    if (cue && cue.start !== null) {
      out.push({
        time: cue.start,
        end: cue.end ?? null,
        token: cueToken(index),
        label: String(index + 1),
        hot: true,
      });
    }
  });
  for (const cue of markers.memoryCues) {
    if (cue.start === null) continue;
    out.push({ time: cue.start, end: cue.end ?? null, token: "fg-muted", label: "", hot: false });
  }
  return out;
}

/** Draws a cue flag whose pole starts at `(x, top)`. */
export function drawCueFlag(
  ctx: CanvasRenderingContext2D,
  colors: VizColors,
  mark: CueMark,
  x: number,
  top: number,
  dpr: number,
): void {
  const fill = colors.color[mark.token];
  const flagH = 12 * dpr;
  const flagW = (mark.label ? 11 : 7) * dpr;
  ctx.fillStyle = fill;
  ctx.fillRect(x, top, flagW, flagH);
  if (mark.label) {
    ctx.fillStyle = colors.color["cue-fg"];
    ctx.font = `600 ${9 * dpr}px ${colors.font}`;
    ctx.textBaseline = "middle";
    ctx.textAlign = "center";
    ctx.fillText(mark.label, x + flagW / 2, top + flagH / 2 + dpr * 0.5);
  }
}
