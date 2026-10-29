import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Button, IconButton, Input, Select, SelectTrigger, Slider, Switch } from "../index";
import { Tabs, TabsList, TabsTrigger, Toggle, ToggleGroup, ToggleGroupItem } from "../index";

const FOCUS = "focus-visible:outline-focus-ring";

function focusable(container: HTMLElement): HTMLElement[] {
  return [
    ...container.querySelectorAll<HTMLElement>(
      "button, input, [role=slider], [role=switch], [role=tab], [role=combobox]",
    ),
  ];
}

describe("visible focus style", () => {
  const cases: [string, React.ReactElement][] = [
    ["Button", <Button key="b">Go</Button>],
    ["IconButton", <IconButton key="i" aria-label="Go" />],
    ["Input", <Input key="in" aria-label="Name" />],
    ["Switch", <Switch key="s" aria-label="Sync" />],
    [
      "Select",
      <Select key="se">
        <SelectTrigger aria-label="Out" />
      </Select>,
    ],
    [
      "Tabs",
      <Tabs key="t" defaultValue="a">
        <TabsList>
          <TabsTrigger value="a">A</TabsTrigger>
        </TabsList>
      </Tabs>,
    ],
    ["Toggle", <Toggle key="to" aria-label="Loop" />],
    [
      "ToggleGroup",
      <ToggleGroup key="tg" type="single">
        <ToggleGroupItem value="1">1</ToggleGroupItem>
      </ToggleGroup>,
    ],
    ["Slider", <Slider key="sl" defaultValue={[1]} aria-label="Vol" />],
  ];

  it.each(cases)("%s uses the focus ring token", (_name, element) => {
    const { container } = render(element);
    const targets = focusable(container);
    expect(targets.length).toBeGreaterThan(0);
    for (const el of targets) {
      expect(el.className).toContain(FOCUS);
    }
  });
});
