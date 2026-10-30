import { useEffect, useRef, type RefObject } from "react";
import { onThemeChange, readColors, type VizColors } from "./colors";

/** What a draw callback receives each animation frame. */
export interface CanvasFrame {
  ctx: CanvasRenderingContext2D;
  /** Canvas size in device pixels. */
  width: number;
  height: number;
  /** Device pixels per CSS pixel. */
  dpr: number;
  /** Token colours. The object changes identity when the theme changes. */
  colors: VizColors;
  /** `performance.now()` of this frame, ms. */
  now: number;
  /** True when size, colours or DPR changed since the last call: redraw even if nothing else moved. */
  invalidated: boolean;
}

/**
 * Owns a canvas: sizes it to its CSS box times DPR, keeps token colours
 * current, and calls `draw` from `requestAnimationFrame`. React never renders
 * per frame; `draw` is read through a ref so it can close over fresh props.
 * `draw` should return early when nothing changed.
 */
export function useCanvasLoop(
  draw: (frame: CanvasFrame) => void,
): RefObject<HTMLCanvasElement | null> {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const drawRef = useRef(draw);
  useEffect(() => {
    drawRef.current = draw;
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d") ?? null;
    if (!canvas || !ctx) {
      return undefined;
    }
    let colors = readColors(canvas);
    let invalidated = true;
    let raf = 0;

    const resize = () => {
      const dpr = globalThis.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(canvas.clientWidth * dpr));
      const height = Math.max(1, Math.round(canvas.clientHeight * dpr));
      if (canvas.width !== width || canvas.height !== height) {
        canvas.width = width;
        canvas.height = height;
        invalidated = true;
      }
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    const stopTheme = onThemeChange(canvas, () => {
      colors = readColors(canvas);
      invalidated = true;
    });

    const tick = (now: number) => {
      drawRef.current({
        ctx,
        width: canvas.width,
        height: canvas.height,
        dpr: globalThis.devicePixelRatio || 1,
        colors,
        now,
        invalidated,
      });
      invalidated = false;
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
      stopTheme();
    };
  }, []);

  return canvasRef;
}
