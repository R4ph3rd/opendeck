import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Slider } from "./slider";

describe("Slider", () => {
  it("forwards aria props to every thumb", () => {
    render(
      <Slider
        defaultValue={[10, 20]}
        aria-label="Loop"
        aria-valuetext="10 to 20 beats"
      />,
    );
    const thumbs = screen.getAllByRole("slider");
    expect(thumbs).toHaveLength(2);
    for (const t of thumbs) {
      expect(t.getAttribute("aria-label")).toBe("Loop");
      expect(t.getAttribute("aria-valuetext")).toBe("10 to 20 beats");
    }
  });

  it("forwards aria-labelledby", () => {
    render(
      <>
        <span id="lbl">Tempo</span>
        <Slider defaultValue={[0]} aria-labelledby="lbl" />
      </>,
    );
    expect(screen.getByRole("slider").getAttribute("aria-labelledby")).toBe("lbl");
  });

  it("supports vertical orientation and a track length", () => {
    const { container } = render(
      <Slider defaultValue={[0]} aria-label="Fader" orientation="vertical" trackLength="12rem" />,
    );
    const root = container.firstElementChild as HTMLElement;
    expect(root.getAttribute("data-orientation")).toBe("vertical");
    expect(root.style.getPropertyValue("--slider-length")).toBe("12rem");
    expect(screen.getByRole("slider").getAttribute("aria-orientation")).toBe("vertical");
  });

  it("places a centre detent at the midpoint", () => {
    const { container } = render(
      <Slider defaultValue={[0]} min={-8} max={8} detent="center" aria-label="Tempo" orientation="vertical" />,
    );
    const mark = container.querySelector<HTMLElement>("[data-slider-detent]");
    expect(mark?.style.bottom).toBe("50%");
  });

  it("renders no detent by default", () => {
    const { container } = render(<Slider defaultValue={[0]} aria-label="Vol" />);
    expect(container.querySelector("[data-slider-detent]")).toBeNull();
  });
});
