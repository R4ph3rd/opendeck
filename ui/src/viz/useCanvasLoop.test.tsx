import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useCanvasLoop, type CanvasFrame } from "./useCanvasLoop";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

let fire: () => void = () => undefined;
class FakeObserver {
  constructor(cb: () => void) {
    fire = cb;
  }
  observe() {}
  disconnect() {}
}

function Probe({ onDraw }: { onDraw: (f: CanvasFrame) => void }) {
  const ref = useCanvasLoop(onDraw);
  return <canvas ref={ref} data-testid="c" />;
}

describe("useCanvasLoop resize", () => {
  it("resizes the bitmap to the visual box times DPR and flags a redraw", () => {
    const ctx = { imageSmoothingEnabled: true } as unknown as CanvasRenderingContext2D;
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(ctx as never);
    vi.stubGlobal("ResizeObserver", FakeObserver);
    vi.stubGlobal("devicePixelRatio", 2);
    let cssW = 720;
    let cssH = 450;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
      () => ({ width: cssW, height: cssH }) as DOMRect,
    );
    const frames: Array<() => void> = [];
    vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
      frames.push(() => cb(0));
      return frames.length;
    });
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
    const seen: CanvasFrame[] = [];
    const { getByTestId } = render(<Probe onDraw={(f) => seen.push({ ...f })} />);
    const canvas = getByTestId("c") as HTMLCanvasElement;
    expect([canvas.width, canvas.height]).toEqual([1440, 900]);
    expect(ctx.imageSmoothingEnabled).toBe(false);

    act(() => frames.shift()?.());
    expect(seen.at(-1)?.invalidated).toBe(true);
    act(() => frames.shift()?.());
    expect(seen.at(-1)?.invalidated).toBe(false);

    // Window grows to 2560x1440 device px: no stale size.
    cssW = 1280;
    cssH = 720;
    fire();
    expect([canvas.width, canvas.height]).toEqual([2560, 1440]);
    act(() => frames.shift()?.());
    const last = seen.at(-1);
    expect(last?.invalidated).toBe(true);
    expect([last?.width, last?.height]).toEqual([2560, 1440]);
  });

  it("falls back to the content box where device-pixel-content-box throws (WebKit)", () => {
    const ctx = { imageSmoothingEnabled: true } as unknown as CanvasRenderingContext2D;
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(ctx as never);
    const boxes: Array<string | undefined> = [];
    class WebKitObserver {
      observe(_el: Element, options?: ResizeObserverOptions) {
        boxes.push(options?.box);
        if (options?.box === "device-pixel-content-box") {
          throw new TypeError("Type error");
        }
      }
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", WebKitObserver);
    vi.stubGlobal("requestAnimationFrame", () => 1);
    vi.stubGlobal("cancelAnimationFrame", () => undefined);
    expect(() => render(<Probe onDraw={() => undefined} />)).not.toThrow();
    expect(boxes).toEqual(["device-pixel-content-box", undefined]);
  });
});
