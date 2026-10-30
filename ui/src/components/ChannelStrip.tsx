import { Slider, Toggle, ToggleGroup, ToggleGroupItem } from "@opendeck/design-system";
import type { CrossfaderAssign, EqBand } from "../ipc/bindings";
import { DECK_TEXT, deckLetter } from "../lib/deck";
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
import { CfxControl, SendKnobs } from "./fx/ChannelFx";
import { LevelMeter } from "./LevelMeter";

/** Fader resolution. */
const FADER_STEP = 0.01;

/** EQ knobs run high to low, top to bottom, like a hardware strip. */
const EQ_ORDER: EqBand[] = ["high", "mid", "low"];

const EQ_LABEL: Record<EqBand, string> = { low: "Low", mid: "Mid", high: "High" };

const ASSIGN_LABEL: Record<CrossfaderAssign, string> = { a: "A", thru: "THRU", b: "B" };

interface ChannelStripProps {
  deck: number;
}

/** One mixer channel: trim, three EQ bands, filter, PFL, assign, fader and meter. */
export function ChannelStrip({ deck }: ChannelStripProps) {
  const channel = useMixerStore((s) => s.channels[deck]);
  const letter = deckLetter(deck);
  if (!channel) {
    return null;
  }

  return (
    <section
      aria-label={`Channel ${letter}`}
      className="flex w-28 flex-col items-center gap-3"
    >
      <h3 className={`text-sm font-semibold ${DECK_TEXT[deck] ?? ""}`}>{letter}</h3>
      <Knob
        label="Trim"
        name={`Trim ${letter}`}
        value={channel.trim}
        defaultValue={TRIM_DEFAULT}
        format={formatTrim}
        bipolar
        onChange={(v) => setTrim(deck, v)}
      />
      {EQ_ORDER.map((band) => (
        <Knob
          key={band}
          label={EQ_LABEL[band]}
          name={`${EQ_LABEL[band]} EQ ${letter}`}
          value={channel.eq[EQ_BANDS.indexOf(band)] ?? EQ_DEFAULT}
          defaultValue={EQ_DEFAULT}
          format={formatEq}
          bipolar
          onChange={(v) => setEq(deck, band, v)}
        />
      ))}
      <Knob
        label="Filter"
        name={`Filter ${letter}`}
        value={channel.filter}
        defaultValue={FILTER_DEFAULT}
        min={-1}
        max={1}
        format={formatFilter}
        bipolar
        onChange={(v) => setFilter(deck, v)}
      />
      <CfxControl deck={deck} />
      <SendKnobs deck={deck} />
      <Toggle
        variant="outline"
        size="sm"
        aria-label={`Headphone cue ${letter}`}
        pressed={channel.pfl}
        onPressedChange={(on) => setPfl(deck, on)}
      >
        PFL
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
            aria-label={`Assign ${letter} to ${ASSIGN_LABEL[assign]}`}
            className="px-1"
          >
            {ASSIGN_LABEL[assign]}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <div className="flex gap-3">
        <Slider
          orientation="vertical"
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
