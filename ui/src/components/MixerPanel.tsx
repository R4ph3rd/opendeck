import {
  Panel,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Slider,
  Toggle,
} from "@opendeck/design-system";
import { useRef } from "react";
import { useTelemetryFrame } from "../hooks/useTelemetryFrame";
import type { CrossfaderCurve } from "../ipc/bindings";
import {
  CROSSFADER_DEFAULT,
  HEADPHONE_MIX_DEFAULT,
  LEVEL_DEFAULT,
  formatLevel,
  formatPercent,
} from "../lib/mixer";
import {
  CROSSFADER_CURVES,
  setCrossfader,
  setCrossfaderCurve,
  setHeadphoneLevel,
  setHeadphoneMix,
  setMasterLevel,
  setSplitCue,
  useMixerStore,
} from "../state/mixer";
import { ChannelStrip } from "./ChannelStrip";
import { Knob } from "./Knob";
import { LevelMeter } from "./LevelMeter";

const CURVE_LABEL: Record<CrossfaderCurve, string> = {
  constant_power: "Smooth",
  linear: "Linear",
  sharp: "Cut",
};

const CROSSFADER_STEP = 0.01;

/** BPM of the sync leader (or the first playing deck), written to the DOM per snapshot. */
function MasterBpm() {
  const ref = useRef<HTMLOutputElement>(null);
  useTelemetryFrame((telemetry) => {
    const decks = telemetry?.decks ?? [];
    const lead =
      telemetry?.leader !== null && telemetry?.leader !== undefined
        ? decks[telemetry.leader]
        : undefined;
    const source = lead ?? decks.find((d) => d.loaded && d.playing) ?? decks.find((d) => d.loaded);
    const bpm = source?.bpm ?? null;
    if (ref.current) {
      ref.current.textContent = bpm !== null && bpm > 0 ? bpm.toFixed(2) : "--";
    }
  });
  return (
    <div className="flex h-8 shrink-0 items-baseline justify-center gap-1.5 border-b border-border px-2 pt-1">
      <span className="text-xs font-semibold uppercase text-fg-muted">Master</span>
      <output
        ref={ref}
        aria-label="Master BPM"
        className="font-mono text-xl font-medium tabular-nums text-fg"
        data-testid="master-bpm"
      >
        --
      </output>
    </div>
  );
}

/**
 * Slim centre mixer: master BPM, two channel strips around the master and
 * headphone section, and the crossfader at the bottom.
 */
export function MixerPanel() {
  const crossfader = useMixerStore((s) => s.crossfader);
  const curve = useMixerStore((s) => s.curve);
  const master = useMixerStore((s) => s.master);
  const headphone = useMixerStore((s) => s.headphone);
  const headphoneMix = useMixerStore((s) => s.headphoneMix);
  const splitCue = useMixerStore((s) => s.splitCue);

  return (
    <Panel flush aria-label="Mixer" className="size-full min-w-0">
      <MasterBpm />
      <div className="flex min-h-0 flex-1 divide-x divide-border">
        <ChannelStrip deck={0} />
        <section
          aria-label="Master and headphones"
          className="flex min-h-0 w-20 shrink-0 flex-col items-center gap-1 px-1 py-2"
        >
          <Knob
            compact
            label="MST"
            name="Master level"
            value={master}
            defaultValue={LEVEL_DEFAULT}
            format={formatLevel}
            onChange={setMasterLevel}
          />
          <Knob
            compact
            label="PHN"
            name="Headphone level"
            value={headphone}
            defaultValue={LEVEL_DEFAULT}
            format={formatLevel}
            onChange={setHeadphoneLevel}
          />
          <Knob
            compact
            label="MIX"
            name="Headphone mix"
            value={headphoneMix}
            defaultValue={HEADPHONE_MIX_DEFAULT}
            format={formatPercent}
            onChange={setHeadphoneMix}
          />
          <Toggle
            size="sm"
            aria-label="Split cue"
            pressed={splitCue}
            className="rounded-sm bg-surface-raised px-1.5 text-xs font-bold data-[state=on]:bg-accent data-[state=on]:text-accent-fg"
            onPressedChange={setSplitCue}
          >
            SPLIT
          </Toggle>
          <div className="min-h-14 w-full flex-1 justify-items-center [&>div]:mx-auto">
            <LevelMeter source="master" label="Level master" />
          </div>
        </section>
        <ChannelStrip deck={1} />
      </div>
      <div className="flex h-8 shrink-0 items-center gap-2 border-t border-border px-2">
        <Slider
          min={-1}
          max={1}
          step={CROSSFADER_STEP}
          detent="center"
          value={[crossfader]}
          aria-label="Crossfader"
          aria-valuetext={formatPercent((crossfader + 1) / 2)}
          onValueChange={([value]) => setCrossfader(value ?? CROSSFADER_DEFAULT)}
          onDoubleClick={() => setCrossfader(CROSSFADER_DEFAULT)}
        />
        <Select
          value={curve}
          onValueChange={(value) => setCrossfaderCurve(value as CrossfaderCurve)}
        >
          <SelectTrigger
            className="h-6 w-20 shrink-0 rounded-sm border-0 bg-surface-raised px-1.5 text-xs"
            aria-label="Crossfader curve"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {CROSSFADER_CURVES.map((c) => (
              <SelectItem key={c} value={c} aria-label={`Curve ${CURVE_LABEL[c]}`}>
                {CURVE_LABEL[c]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </Panel>
  );
}
