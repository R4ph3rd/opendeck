import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
  Callout,
  ColorSwatchPicker,
  Combobox,
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
  KeyValueList,
  ProgressBar,
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  SegmentedControl,
  SettingsNav,
  SettingsRow,
  SettingsSection,
  Skeleton,
  StatusDot,
} from "../index";

const FOCUS = "focus-visible:outline-focus-ring";

describe("SegmentedControl", () => {
  const options = [
    { value: "8", label: "8%" },
    { value: "16", label: "16%" },
    { value: "50", label: "50%", disabled: true },
  ];
  it("is a radio group with one checked option and the focus ring", () => {
    const onValueChange = vi.fn();
    render(
      <SegmentedControl
        aria-label="Tempo range"
        options={options}
        defaultValue="8"
        onValueChange={onValueChange}
      />,
    );
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(3);
    expect(radios[0]?.getAttribute("aria-checked")).toBe("true");
    for (const r of radios) expect(r.className).toContain(FOCUS);
    fireEvent.click(radios[1] as HTMLElement);
    expect(onValueChange).toHaveBeenCalledWith("16");
    expect((radios[2] as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("ColorSwatchPicker", () => {
  it("offers the 8 cue colors, labelled, and reports the pick", () => {
    const onValueChange = vi.fn();
    render(<ColorSwatchPicker defaultValue={1} onValueChange={onValueChange} />);
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(8);
    for (const r of radios) {
      expect(r.getAttribute("aria-label")).toMatch(/cue \d/);
      expect(r.className).toContain(FOCUS);
    }
    expect(radios[1]?.className).toContain("bg-cue-2");
    expect(radios[0]?.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(radios[4] as HTMLElement);
    expect(onValueChange).toHaveBeenCalledWith(5);
  });
});

describe("StatusDot", () => {
  it("exposes label and state as text", () => {
    const { rerender } = render(<StatusDot label="SYNC" tone="sync" />);
    expect(screen.getByRole("status").textContent).toBe("SYNC, on");
    rerender(<StatusDot label="SYNC" tone="sync" state="off" />);
    expect(screen.getByRole("status").textContent).toBe("SYNC, off");
  });
  it("blinks through the motion token and stops under reduced motion", () => {
    const { container } = render(<StatusDot label="LOOP" state="blink" tone="loop" />);
    const led = container.querySelector("[aria-hidden]") as HTMLElement;
    expect(led.className).toContain("var(--duration-blink)");
    expect(led.className).toContain("motion-reduce:animate-none");
  });
  it("off state is a hollow ring", () => {
    const { container } = render(<StatusDot state="off" />);
    expect((container.querySelector("[aria-hidden]") as HTMLElement).className).toContain(
      "border-border-strong",
    );
  });
});

describe("ProgressBar", () => {
  it("determinate reports its value", () => {
    render(<ProgressBar aria-label="Import" value={40} />);
    const bar = screen.getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("40");
    expect(bar.getAttribute("data-state")).toBe("loading");
  });
  it("indeterminate has no value", () => {
    render(<ProgressBar aria-label="Import" />);
    const bar = screen.getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBeNull();
    expect(bar.getAttribute("data-state")).toBe("indeterminate");
  });
});

describe("Skeleton", () => {
  it("is hidden from assistive tech and respects reduced motion", () => {
    const { container } = render(<Skeleton className="h-4 w-20" />);
    const el = container.firstElementChild as HTMLElement;
    expect(el.getAttribute("aria-hidden")).toBe("true");
    expect(el.className).toContain("motion-reduce:animate-none");
  });
});

describe("Callout", () => {
  it("uses alert for danger and status otherwise", () => {
    const { rerender } = render(<Callout variant="danger" title="Failed">Disk full</Callout>);
    expect(screen.getByRole("alert").textContent).toContain("Disk full");
    rerender(<Callout variant="warning" title="Heads up">Slow</Callout>);
    expect(screen.getByRole("status").textContent).toContain("Heads up");
  });
});

describe("Breadcrumb", () => {
  it("is a labelled nav with the current page marked", () => {
    render(
      <Breadcrumb>
        <BreadcrumbList>
          <BreadcrumbItem>
            <BreadcrumbLink href="/">Music</BreadcrumbLink>
          </BreadcrumbItem>
          <BreadcrumbSeparator />
          <BreadcrumbItem>
            <BreadcrumbPage>House</BreadcrumbPage>
          </BreadcrumbItem>
        </BreadcrumbList>
      </Breadcrumb>,
    );
    expect(screen.getByRole("navigation", { name: "Breadcrumb" })).toBeTruthy();
    expect(screen.getByText("House").getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "Music" }).className).toContain(FOCUS);
  });
});

describe("KeyValueList", () => {
  it("renders term and description pairs", () => {
    const { container } = render(
      <KeyValueList items={[{ label: "Sample rate", value: "44100 Hz" }, { label: "Format", value: "FLAC" }]} />,
    );
    expect(container.querySelectorAll("dt")).toHaveLength(2);
    expect(container.querySelector("dd")?.textContent).toBe("44100 Hz");
  });
});

describe("Settings", () => {
  it("nav marks the active section and reports clicks", () => {
    const onValueChange = vi.fn();
    render(
      <SettingsNav
        items={[{ id: "audio", label: "Audio" }, { id: "midi", label: "MIDI" }]}
        value="audio"
        onValueChange={onValueChange}
      />,
    );
    const buttons = screen.getAllByRole("button");
    expect(buttons[0]?.getAttribute("aria-current")).toBe("page");
    expect(buttons[1]?.getAttribute("aria-current")).toBeNull();
    expect(buttons[1]?.className).toContain(FOCUS);
    fireEvent.click(buttons[1] as HTMLElement);
    expect(onValueChange).toHaveBeenCalledWith("midi");
  });
  it("sections are labelled by their heading", () => {
    render(
      <SettingsSection id="audio" title="Audio" description="Output">
        <SettingsRow title="Device" control={<span>x</span>} />
      </SettingsSection>,
    );
    expect(screen.getByRole("region", { name: "Audio" })).toBeTruthy();
  });
});

describe("Resizable", () => {
  it("renders panels and a focusable separator", () => {
    render(
      <ResizablePanelGroup orientation="horizontal" onLayoutChanged={() => {}}>
        <ResizablePanel id="a">A</ResizablePanel>
        <ResizableHandle withGrip />
        <ResizablePanel id="b">B</ResizablePanel>
      </ResizablePanelGroup>,
    );
    const sep = screen.getByRole("separator");
    expect(sep.className).toContain(FOCUS);
    expect(sep.getAttribute("tabindex")).toBe("0");
    expect(screen.getByText("A")).toBeTruthy();
  });
});

describe("Command and Combobox", () => {
  it("filters command items by the query", () => {
    render(
      <Command>
        <CommandInput placeholder="Search" />
        <CommandList>
          <CommandEmpty>Nothing</CommandEmpty>
          <CommandItem>Load to deck A</CommandItem>
          <CommandItem>Open settings</CommandItem>
        </CommandList>
      </Command>,
    );
    fireEvent.change(screen.getByPlaceholderText("Search"), { target: { value: "settings" } });
    expect(screen.queryByText("Load to deck A")).toBeNull();
    expect(screen.getByText("Open settings")).toBeTruthy();
    fireEvent.change(screen.getByPlaceholderText("Search"), { target: { value: "zzz" } });
    expect(screen.getByText("Nothing")).toBeTruthy();
  });

  it("combobox opens, selects and closes", () => {
    const onValueChange = vi.fn();
    render(
      <Combobox
        aria-label="Output"
        options={[{ value: "a", label: "Built-in" }, { value: "b", label: "USB DAC" }]}
        onValueChange={onValueChange}
      />,
    );
    const trigger = screen.getByRole("combobox", { name: "Output" });
    expect(trigger.className).toContain(FOCUS);
    fireEvent.click(trigger);
    fireEvent.click(screen.getByText("USB DAC"));
    expect(onValueChange).toHaveBeenCalledWith("b");
  });
});
