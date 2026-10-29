import {
  Panel,
  PanelBody,
  PanelHeader,
  Slider,
  Toggle,
  ToggleGroup,
  ToggleGroupItem,
} from "@opendeck/design-system";
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

/** Centre mixer: two channel strips, crossfader, master and headphone section. */
export function MixerPanel() {
  const crossfader = useMixerStore((s) => s.crossfader);
  const curve = useMixerStore((s) => s.curve);
  const master = useMixerStore((s) => s.master);
  const headphone = useMixerStore((s) => s.headphone);
  const headphoneMix = useMixerStore((s) => s.headphoneMix);
  const splitCue = useMixerStore((s) => s.splitCue);

  return (
    <Panel aria-label="Mixer" className="w-72 shrink-0">
      <PanelHeader title="Mixer" />
      <PanelBody className="flex flex-col items-center gap-4">
        <div className="flex gap-3">
          <ChannelStrip deck={0} />
          <ChannelStrip deck={1} />
        </div>
        <div className="flex w-full flex-col items-center gap-2">
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
          <ToggleGroup
            type="single"
            aria-label="Crossfader curve"
            value={curve}
            onValueChange={(value) => {
              if (value) {
                setCrossfaderCurve(value as CrossfaderCurve);
              }
            }}
          >
            {CROSSFADER_CURVES.map((c) => (
              <ToggleGroupItem
                key={c}
                value={c}
                size="sm"
                aria-label={`Curve ${CURVE_LABEL[c]}`}
              >
                {CURVE_LABEL[c]}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
        <section aria-label="Master and headphones" className="flex w-full items-end gap-3">
          <div className="flex flex-1 flex-col gap-2">
            <Knob
              label="Master"
              name="Master level"
              value={master}
              defaultValue={LEVEL_DEFAULT}
              format={formatLevel}
              onChange={setMasterLevel}
            />
            <Knob
              label="Phones"
              name="Headphone level"
              value={headphone}
              defaultValue={LEVEL_DEFAULT}
              format={formatLevel}
              onChange={setHeadphoneLevel}
            />
            <Knob
              label="Cue / master"
              name="Headphone mix"
              value={headphoneMix}
              defaultValue={HEADPHONE_MIX_DEFAULT}
              format={formatPercent}
              onChange={setHeadphoneMix}
            />
            <Toggle
              variant="outline"
              size="sm"
              aria-label="Split cue"
              pressed={splitCue}
              onPressedChange={setSplitCue}
            >
              SPLIT
            </Toggle>
          </div>
          <div className="h-32">
            <LevelMeter source="master" label="Level master" />
          </div>
        </section>
      </PanelBody>
    </Panel>
  );
}
