import type { Meta, StoryObj } from "@storybook/react-vite";
import { useCallback } from "react";
import type { StereoMeter } from "../ipc/bindings";
import { LevelMeterCanvas } from "./LevelMeter";
import { mockMeter } from "./mock";

const meta = {
  title: "Viz/LevelMeter",
  component: LevelMeterCanvas,
  parameters: {
    docs: {
      description: {
        component:
          "Stereo meter on a canvas: RMS as the wide dim bar, peak as the narrow bar, a peak-hold tick and a latching clip lamp (3 s). " +
          "Zones: green, amber from -12 dBFS, red from -3 dBFS, over a -60 dBFS floor. Colours are the `--meter-*` tokens.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

function Strip({ label, clip = false, fixed }: { label: string; clip?: boolean; fixed?: StereoMeter }) {
  const read = useCallback(
    () => fixed ?? mockMeter(performance.now(), { clip }),
    [clip, fixed],
  );
  return (
    <div className="flex flex-col items-center gap-1">
      <div className="h-40">
        <LevelMeterCanvas label={label} read={read} />
      </div>
      <span className="text-xs text-fg-muted">{label}</span>
    </div>
  );
}

export const Music: Story = { render: () => <div className="p-4"><Strip label="Level A" /></div> };
export const Clipping: Story = { render: () => <div className="p-4"><Strip label="Level B" clip /></div> };
export const Steps: Story = {
  render: () => (
    <div className="flex gap-4 p-4">
      {[0.01, 0.1, 0.25, 0.5, 0.71, 0.9, 1].map((peak) => (
        <Strip
          key={peak}
          label={`${peak}`}
          fixed={{ peak_l: peak, peak_r: peak, rms_l: peak * 0.6, rms_r: peak * 0.6 }}
        />
      ))}
    </div>
  ),
};
export const Silent: Story = {
  render: () => (
    <div className="p-4">
      <Strip label="Silent" fixed={{ peak_l: 0, peak_r: 0, rms_l: 0, rms_r: 0 }} />
    </div>
  ),
};
