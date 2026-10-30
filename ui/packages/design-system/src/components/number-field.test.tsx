import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { NumberField, normalizeNumber } from "./number-field";

function setup(props: Partial<React.ComponentProps<typeof NumberField>> = {}) {
  const onValueChange = vi.fn();
  const utils = render(
    <NumberField
      aria-label="BPM"
      defaultValue={120}
      min={20}
      max={300}
      step={0.1}
      largeStep={1}
      onValueChange={onValueChange}
      {...props}
    />,
  );
  const input = screen.getByRole("spinbutton") as HTMLInputElement;
  return { ...utils, input, onValueChange };
}

describe("normalizeNumber", () => {
  it("clamps and rounds", () => {
    expect(normalizeNumber(0.1 + 0.2, 0, 10, 1)).toBe(0.3);
    expect(normalizeNumber(999, 20, 300, 1)).toBe(300);
    expect(normalizeNumber(-5, 20, 300, 1)).toBe(20);
  });
});

describe("NumberField", () => {
  it("exposes spinbutton semantics and precision from step", () => {
    const { input } = setup();
    expect(input.value).toBe("120.0");
    expect(input.getAttribute("aria-valuenow")).toBe("120");
    expect(input.getAttribute("aria-valuemin")).toBe("20");
    expect(input.getAttribute("aria-valuemax")).toBe("300");
    expect(input.getAttribute("aria-label")).toBe("BPM");
  });

  it("steps with arrows, shift and alt", () => {
    const { input, onValueChange } = setup();
    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(onValueChange).toHaveBeenLastCalledWith(120.1);
    fireEvent.keyDown(input, { key: "ArrowUp", shiftKey: true });
    expect(onValueChange).toHaveBeenLastCalledWith(121.1);
    fireEvent.keyDown(input, { key: "ArrowDown", altKey: true });
    expect(onValueChange).toHaveBeenLastCalledWith(121.1);
  });

  it("jumps to limits with Home and End", () => {
    const { input, onValueChange } = setup();
    fireEvent.keyDown(input, { key: "End" });
    expect(onValueChange).toHaveBeenLastCalledWith(300);
    fireEvent.keyDown(input, { key: "Home" });
    expect(onValueChange).toHaveBeenLastCalledWith(20);
  });

  it("commits typed text on Enter and clamps it", () => {
    const { input, onValueChange } = setup();
    fireEvent.change(input, { target: { value: "999" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onValueChange).toHaveBeenLastCalledWith(300);
    expect(input.value).toBe("300.0");
  });

  it("accepts a decimal comma and ignores garbage on blur", () => {
    const { input, onValueChange } = setup();
    fireEvent.change(input, { target: { value: "128,5" } });
    fireEvent.blur(input);
    expect(onValueChange).toHaveBeenLastCalledWith(128.5);
    fireEvent.change(input, { target: { value: "abc" } });
    fireEvent.blur(input);
    expect(input.value).toBe("128.5");
  });

  it("Escape discards a draft", () => {
    const { input, onValueChange } = setup();
    fireEvent.change(input, { target: { value: "200" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input.value).toBe("120.0");
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it("steps with the wheel only while focused", () => {
    const { input, onValueChange } = setup();
    fireEvent.wheel(input, { deltaY: -100 });
    expect(onValueChange).not.toHaveBeenCalled();
    input.focus();
    fireEvent.wheel(input, { deltaY: -100 });
    expect(onValueChange).toHaveBeenLastCalledWith(120.1);
    fireEvent.wheel(input, { deltaY: 100 });
    expect(onValueChange).toHaveBeenLastCalledWith(120);
  });

  it("scrubs by dragging the grip", () => {
    const { container, input, onValueChange } = setup({ step: 1, largeStep: 10, scrubPixels: 4 });
    const grip = container.querySelector("[data-number-scrub]") as HTMLElement;
    fireEvent.pointerDown(grip, { clientX: 100, pointerId: 1 });
    fireEvent.pointerMove(grip, { clientX: 112, pointerId: 1 });
    expect(onValueChange).toHaveBeenLastCalledWith(123);
    fireEvent.pointerMove(grip, { clientX: 92, pointerId: 1 });
    expect(onValueChange).toHaveBeenLastCalledWith(118);
    fireEvent.pointerUp(grip, { pointerId: 1 });
    fireEvent.pointerMove(grip, { clientX: 200, pointerId: 1 });
    expect(input.value).toBe("118");
  });

  it("works controlled", () => {
    const { rerender, input } = setup({ value: 90 });
    expect(input.value).toBe("90.0");
    rerender(<NumberField aria-label="BPM" value={95} step={0.1} />);
    expect((screen.getByRole("spinbutton") as HTMLInputElement).value).toBe("95.0");
  });

  it("ignores input when disabled", () => {
    const { input } = setup({ disabled: true });
    expect(input.disabled).toBe(true);
  });
});
