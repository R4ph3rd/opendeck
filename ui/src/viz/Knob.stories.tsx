import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { Knob } from "./Knob";

const meta = {
  title: "Viz/Knob",
  component: Knob,
  parameters: {
    docs: {
      description: {
        component:
          "Canvas knob. Drag vertically (Shift for fine), wheel, arrow, Page, Home and End keys, double click to reset. " +
          "A bipolar knob grows its arc from the centre and snaps onto it. " +
          "The accessible control is a visually hidden range input with `aria-valuetext`.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

function Controlled({
  min = 0,
  max = 1,
  start,
  bipolar = false,
  label,
  disabled = false,
  format,
}: {
  min?: number;
  max?: number;
  start: number;
  bipolar?: boolean;
  label: string;
  disabled?: boolean;
  format?: (v: number) => string;
}) {
  const [value, setValue] = useState(start);
  return (
    <div className="w-16 p-4">
      <Knob
        label={label}
        value={value}
        defaultValue={start}
        min={min}
        max={max}
        bipolar={bipolar}
        disabled={disabled}
        {...(format ? { format } : {})}
        onChange={setValue}
      />
    </div>
  );
}

export const Unipolar: Story = {
  render: () => <Controlled label="Master" start={0.75} format={(v) => `${Math.round(v * 100)}%`} />,
};
export const Bipolar: Story = {
  render: () => (
    <Controlled
      label="Filter"
      start={0}
      min={-1}
      max={1}
      bipolar
      format={(v) => (v === 0 ? "off" : `${v < 0 ? "LP" : "HP"} ${Math.round(Math.abs(v) * 100)}%`)}
    />
  ),
};
export const Disabled: Story = {
  render: () => <Controlled label="Trim" start={0.5} bipolar disabled />,
};
export const EqStrip: Story = {
  render: () => (
    <div className="flex w-56 gap-2 p-4">
      <Controlled label="High" start={0.5} bipolar />
      <Controlled label="Mid" start={0.5} bipolar />
      <Controlled label="Low" start={0.5} bipolar />
    </div>
  ),
};
