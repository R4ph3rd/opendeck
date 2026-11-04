import { Slider, Toggle, ToggleGroup, ToggleGroupItem, cn } from "@opendeck/design-system";
import type { CrossfaderAssign, EqBand } from "../ipc/bindings";
import { DECK_FILL, deckLetter } from "../lib/deck";
import {
  EQ_DEFAULT,
  FADER_DEFAULT,
  FILTER_DEFAULT,
  TRIM_DEFAULT,
  formatEq,
  formatFilter,
  formatTrim,
} from "../lib/mixer";
import {
  CROSSFADER_ASSIGNS,
  EQ_BANDS,
  setChannelFader,
  setCrossfaderAssign,
  setEq,
  setFilter,
  setPfl,
  setTrim,
  useMixerStore,
} from "../state/mixer";
import { Knob } from "./Knob";
import { CfxKnob, CfxType, SendKnobs } from "./fx/ChannelFx";
import { LevelMeter } from "./LevelMeter";

/** Fader resolution. */
const FADER_STEP = 0.01;

/** EQ knobs run high to low, top to bottom, like a hardware strip. */
const EQ_ORDER: EqBand[] = ["high", "mid", "low"];

const EQ_LABEL: Record<EqBand, string> = {
  low: "Low",
  mid: "Mid",
  high: "High",
};
const EQ_SHORT: Record<EqBand, string> = { low: "LOW", mid: "MID", high: "HI" };

const ASSIGN_LABEL: Record<CrossfaderAssign, string> = {
  a: "A",
  thru: "THRU",
  b: "B",
};
const ASSIGN_SHORT: Record<CrossfaderAssign, string> = {
  a: "A",
  thru: "T",
  b: "B",
};

interface ChannelStripProps {
  deck: number;
}

/**
 * One mixer channel: trim, three EQ bands, filter, Sound Color FX and sends
 * as a two-column knob grid, then CUE (PFL) with crossfader assign, the
 * channel fader and a stereo meter.
 */
export function ChannelStrip({ deck }: ChannelStripProps) {
  const channel = useMixerStore((s) => s.channels[deck]);
  const letter = deckLetter(deck);
  if (!channel) {
    return null;
  }

  return (
    <section
      aria-label={`Channel ${letter}`}
      className="relative flex min-h-0 min-w-0 flex-1 flex-col items-center gap-1 px-0.5 pb-1 pt-2"
    >
      <span aria-hidden className={cn("absolute inset-x-0 top-0 h-0.5", DECK_FILL[deck] ?? "")} />
      <div className="grid w-full grid-cols-2 gap-x-1 gap-y-0.5">
        <Knob
          compact
          label="TRM"
          name={`Trim ${letter}`}
          value={channel.trim}
          defaultValue={TRIM_DEFAULT}
          format={formatTrim}
          bipolar
          onChange={(v) => setTrim(deck, v)}
        />
        <Knob
          compact
          label="FLT"
          name={`Filter ${letter}`}
          value={channel.filter}
          defaultValue={FILTER_DEFAULT}
          min={-1}
          max={1}
          format={formatFilter}
          bipolar
          onChange={(v) => setFilter(deck, v)}
        />
        {EQ_ORDER.map((band) => (
          <Knob
            key={band}
            compact
            label={EQ_SHORT[band]}
            name={`${EQ_LABEL[band]} EQ ${letter}`}
            value={channel.eq[EQ_BANDS.indexOf(band)] ?? EQ_DEFAULT}
            defaultValue={EQ_DEFAULT}
            format={formatEq}
            bipolar
            onChange={(v) => setEq(deck, band, v)}
          />
        ))}
        <CfxKnob deck={deck} compact />
        <SendKnobs deck={deck} />
      </div>
      <CfxType deck={deck} />
      <div className="flex w-full items-center justify-between gap-0.5">
        <Toggle
          size="sm"
          aria-label={`Headphone cue ${letter}`}
          pressed={channel.pfl}
          className="rounded-sm bg-surface-raised px-1.5 text-xs font-bold data-[state=on]:bg-accent data-[state=on]:text-accent-fg"
          onPressedChange={(on) => setPfl(deck, on)}
        >
          CUE
        </Toggle>
        <ToggleGroup
          type="single"
          aria-label={`Crossfader assign ${letter}`}
          value={channel.assign}
          onValueChange={(value) => {
            if (value) {
              setCrossfaderAssign(deck, value as CrossfaderAssign);
            }
          }}
        >
          {CROSSFADER_ASSIGNS.map((assign) => (
            <ToggleGroupItem
              key={assign}
              value={assign}
              size="sm"
              title={ASSIGN_LABEL[assign]}
              aria-label={`Assign ${letter} to ${ASSIGN_LABEL[assign]}`}
              className="min-w-5 rounded-sm px-0.5 text-xs"
            >
              {ASSIGN_SHORT[assign]}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      <div className="flex min-h-14 w-full flex-1 justify-center gap-3">
        <Slider
          orientation="vertical"
          trackLength="100%"
          min={0}
          max={1}
          step={FADER_STEP}
          value={[channel.fader]}
          aria-label={`Channel fader ${letter}`}
          aria-valuetext={`${Math.round(channel.fader * 100)}%`}
          onValueChange={([value]) => setChannelFader(deck, value ?? FADER_DEFAULT)}
          onDoubleClick={() => setChannelFader(deck, FADER_DEFAULT)}
        />
        <LevelMeter source={{ deck }} label={`Level ${letter}`} />
      </div>
    </section>
  );
}
