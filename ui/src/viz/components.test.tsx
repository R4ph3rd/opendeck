import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EMPTY_DECK_FRAME, type DeckFrame } from "./deckFrame";
import { JogWheel } from "./JogWheel";
import { Knob } from "./Knob";
import { NO_MARKERS } from "./markers";
import { fractionAt, Overview } from "./Overview";
import { PerformancePads, type PadSpec } from "./PerformancePads";
import { Waveform } from "./Waveform";
import { SEARCH_BEATS_PER_REV } from "./jogMath";

afterEach(cleanup);

/** Runs queued animation frames immediately. */
function flushFrames() {
  vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
    cb(0);
    return 1;
  });
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
}

const rect = (left: number, top: number, size: number) =>
  ({ left, top, width: size, height: size, right: left + size, bottom: top + size, x: left, y: top }) as DOMRect;

describe("Knob", () => {
  const setup = (over: Partial<Parameters<typeof Knob>[0]> = {}) => {
    const onChange = vi.fn();
    render(
      <Knob label="Low" name="Low EQ A" value={0.5} defaultValue={0.5} onChange={onChange} {...over} />,
    );
    return { onChange, slider: screen.getByRole("slider", { name: "Low EQ A" }) };
  };

  it("exposes slider semantics", () => {
    const { slider } = setup({ format: (v) => `${Math.round(v * 100)}%` });
    expect(slider.getAttribute("aria-valuetext")).toBe("50%");
    expect((slider as HTMLInputElement).value).toBe("0.5");
  });

  it("moves with the arrow, page and home keys", () => {
    const { onChange, slider } = setup();
    fireEvent.keyDown(slider, { key: "ArrowUp" });
    expect(onChange).toHaveBeenLastCalledWith(0.51);
    fireEvent.keyDown(slider, { key: "PageDown" });
    expect(onChange).toHaveBeenLastCalledWith(0.4);
    fireEvent.keyDown(slider, { key: "Home" });
    expect(onChange).toHaveBeenLastCalledWith(0);
  });

  it("resets on double click", () => {
    const { onChange, slider } = setup({ value: 0.9 });
    fireEvent.doubleClick(slider);
    expect(onChange).toHaveBeenCalledWith(0.5);
  });

  it("drags vertically, up raises the value", () => {
    const { onChange, slider } = setup();
    const dial = slider.parentElement as HTMLElement;
    fireEvent.pointerDown(dial, { pointerId: 1, clientY: 200 });
    fireEvent.pointerMove(dial, { pointerId: 1, clientY: 120 });
    // 80 px up of 160 px travel is half the range.
    expect(onChange).toHaveBeenLastCalledWith(1);
    fireEvent.pointerUp(dial, { pointerId: 1 });
    fireEvent.pointerMove(dial, { pointerId: 1, clientY: 0 });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("follows the wheel", () => {
    const { onChange, slider } = setup();
    fireEvent.wheel(slider.parentElement as HTMLElement, { deltaY: -100 });
    expect(onChange).toHaveBeenLastCalledWith(0.51);
  });

  it("ignores input when disabled", () => {
    const { onChange, slider } = setup({ disabled: true });
    fireEvent.doubleClick(slider);
    fireEvent.wheel(slider.parentElement as HTMLElement, { deltaY: -100 });
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("PerformancePads", () => {
  const pads: PadSpec[] = [
    { label: "1", color: "cue-1", lit: true },
    { label: "2", color: "cue-2", lit: false, ariaLabel: "Pad two (empty)" },
  ];

  it("reports one press and one release, also when the pointer is lost", () => {
    const onPress = vi.fn();
    const onRelease = vi.fn();
    render(<PerformancePads label="Pads" pads={pads} onPress={onPress} onRelease={onRelease} />);
    const pad = screen.getByRole("button", { name: "1" });
    fireEvent.pointerDown(pad, { pointerId: 1, shiftKey: true });
    fireEvent.pointerDown(pad, { pointerId: 1 });
    expect(pad.dataset["pressed"]).toBe("true");
    fireEvent.lostPointerCapture(pad, { pointerId: 1 });
    fireEvent.pointerUp(pad, { pointerId: 1 });
    expect(onPress.mock.calls).toEqual([[0, { shift: true }]]);
    expect(onRelease.mock.calls).toEqual([[0]]);
    expect(pad.dataset["pressed"]).toBeUndefined();
  });

  it("colours lit pads from tokens and names empty ones", () => {
    render(<PerformancePads label="Pads" pads={pads} onPress={() => undefined} />);
    expect(screen.getByRole("button", { name: "1" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "1" }).getAttribute("style")).toContain("--cue-1");
    expect(screen.getByRole("button", { name: "Pad two (empty)" }).getAttribute("aria-pressed")).toBe("false");
  });

  it("works from the keyboard", () => {
    const onPress = vi.fn();
    const onRelease = vi.fn();
    render(<PerformancePads label="Pads" pads={pads} onPress={onPress} onRelease={onRelease} />);
    const pad = screen.getByRole("button", { name: "1" });
    fireEvent.keyDown(pad, { key: " " });
    fireEvent.keyDown(pad, { key: " ", repeat: true });
    fireEvent.keyUp(pad, { key: " " });
    expect(onPress).toHaveBeenCalledTimes(1);
    expect(onRelease).toHaveBeenCalledTimes(1);
  });
});

describe("JogWheel", () => {
  const setup = () => {
    flushFrames();
    const handlers = { onTouch: vi.fn(), onTurn: vi.fn(), onSearch: vi.fn() };
    render(<JogWheel deck={0} label="Deck A jog wheel" frameSource={() => EMPTY_DECK_FRAME} {...handlers} />);
    const wheel = screen.getByRole("group", { name: "Deck A jog wheel" });
    wheel.getBoundingClientRect = () => rect(0, 0, 200);
    return { wheel, ...handlers };
  };
  afterEach(() => vi.unstubAllGlobals());

  it("touches the platter, sends rotation and releases", () => {
    const { wheel, onTouch, onTurn } = setup();
    // Centre is (100, 100). Start at 3 o'clock on the platter, quarter turn clockwise to 6 o'clock.
    fireEvent.pointerDown(wheel, { pointerId: 1, clientX: 150, clientY: 100 });
    expect(onTouch).toHaveBeenLastCalledWith(true);
    fireEvent.pointerMove(wheel, { pointerId: 1, clientX: 100, clientY: 150 });
    fireEvent.pointerUp(wheel, { pointerId: 1 });
    expect(onTouch).toHaveBeenLastCalledWith(false);
    const total = onTurn.mock.calls.reduce((sum, [r]) => sum + (r as number), 0);
    expect(total).toBeCloseTo(0.25, 6);
  });

  it("nudges from the outer ring without touching", () => {
    const { wheel, onTouch, onTurn } = setup();
    fireEvent.pointerDown(wheel, { pointerId: 1, clientX: 195, clientY: 100 });
    fireEvent.pointerMove(wheel, { pointerId: 1, clientX: 100, clientY: 195 });
    fireEvent.pointerUp(wheel, { pointerId: 1 });
    expect(onTouch).not.toHaveBeenCalled();
    expect(onTurn).toHaveBeenCalled();
  });

  it("searches by beats while shift is held", () => {
    const { wheel, onSearch, onTurn } = setup();
    fireEvent.pointerDown(wheel, { pointerId: 1, clientX: 150, clientY: 100, shiftKey: true });
    fireEvent.pointerMove(wheel, { pointerId: 1, clientX: 100, clientY: 150, shiftKey: true });
    fireEvent.pointerUp(wheel, { pointerId: 1 });
    expect(onTurn).not.toHaveBeenCalled();
    expect(onSearch.mock.calls[0]?.[0]).toBeCloseTo(0.25 * SEARCH_BEATS_PER_REV, 6);
  });

  it("ignores presses outside the wheel", () => {
    const { wheel, onTouch } = setup();
    fireEvent.pointerDown(wheel, { pointerId: 1, clientX: 5, clientY: 5 });
    expect(onTouch).not.toHaveBeenCalled();
  });
});

describe("Overview", () => {
  const frame: DeckFrame = { ...EMPTY_DECK_FRAME, loaded: true, duration: 200 };
  it("maps a click to a position", () => {
    const onSeek = vi.fn();
    render(<Overview label="Pos" level={null} markers={NO_MARKERS} frameSource={() => frame} onSeek={onSeek} />);
    const bar = screen.getByRole("slider", { name: "Pos" });
    bar.getBoundingClientRect = () => ({ left: 100, width: 400 }) as DOMRect;
    fireEvent.pointerDown(bar, { pointerId: 1, clientX: 300 });
    expect(onSeek).toHaveBeenCalledWith(100);
    fireEvent.keyDown(bar, { key: "End" });
    expect(onSeek).toHaveBeenLastCalledWith(200);
  });
  it("clamps the fraction", () => {
    expect(fractionAt(50, { left: 100, width: 400 })).toBe(0);
    expect(fractionAt(900, { left: 100, width: 400 })).toBe(1);
    expect(fractionAt(5, { left: 0, width: 0 })).toBe(0);
  });
});

describe("Waveform", () => {
  const frame: DeckFrame = { ...EMPTY_DECK_FRAME, loaded: true, duration: 300, position: 100 };
  const setup = (zoomIndex = 2) => {
    const onZoomChange = vi.fn();
    const onScrub = vi.fn();
    render(
      <Waveform
        label="Deck A waveform"
        levels={[]}
        markers={NO_MARKERS}
        frameSource={() => frame}
        zoomIndex={zoomIndex}
        onZoomChange={onZoomChange}
        onScrub={onScrub}
      />,
    );
    return { onZoomChange, onScrub, root: screen.getByRole("group", { name: "Deck A waveform" }) };
  };
  afterEach(() => vi.unstubAllGlobals());

  it("zooms with the buttons, keys and wheel", () => {
    const { onZoomChange, root } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Zoom in Deck A waveform" }));
    expect(onZoomChange).toHaveBeenLastCalledWith(1);
    fireEvent.click(screen.getByRole("button", { name: "Zoom out Deck A waveform" }));
    expect(onZoomChange).toHaveBeenLastCalledWith(3);
    fireEvent.keyDown(root, { key: "+" });
    expect(onZoomChange).toHaveBeenLastCalledWith(1);
    fireEvent.keyDown(root, { key: "-" });
    expect(onZoomChange).toHaveBeenLastCalledWith(3);
    fireEvent.wheel(root, { deltaY: -120 });
    expect(onZoomChange).toHaveBeenLastCalledWith(0);
  });

  it("disables zoom at the ends", () => {
    setup(0);
    expect((screen.getByRole("button", { name: "Zoom in Deck A waveform" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("scrubs by dragging, moving the audio with the pointer", () => {
    flushFrames();
    const { onScrub, root } = setup();
    Object.defineProperty(root, "clientWidth", { value: 800 });
    act(() => {
      fireEvent.pointerDown(root, { pointerId: 1, clientX: 400 });
      // Drag right by a quarter of the width: a quarter window (2 s of 8 s) earlier.
      fireEvent.pointerMove(root, { pointerId: 1, clientX: 600 });
      fireEvent.pointerUp(root, { pointerId: 1 });
    });
    expect(onScrub).toHaveBeenLastCalledWith(98);
  });
});
