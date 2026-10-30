import { Slider } from "@opendeck/design-system";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { useCallback, useState } from "react";
import { DECK_BORDER, DECK_TEXT } from "../lib/deck";
import { cueToken } from "./colors";
import { JogWheel } from "./JogWheel";
import { Knob } from "./Knob";
import { LevelMeterCanvas } from "./LevelMeter";
import { MOCK_BPM, mockMeter, type MockDeck } from "./mock";
import { Overview } from "./Overview";
import { PerformancePads, type PadSpec } from "./PerformancePads";
import { storyDeck, storyTrack } from "./storyData";
import { Waveform } from "./Waveform";

const meta = {
  title: "Viz/Layout",
  parameters: {
    docs: {
      description: {
        component:
          "Two decks and a mixer composed from the viz components on synthetic tracks: the zoomed waveform pair on top, " +
          "then per deck an overview, jog wheel and hot cue pads, and a two-channel mixer with EQ knobs, meters and a crossfader. " +
          "No store or engine; every value is local mock state.",
      },
    },
  },
} as Meta;
export default meta;
type Story = StoryObj;

const SECONDS = 360;

const BAND_LABEL = { high: "High", mid: "Mid", low: "Low" } as const;

const hotCues = (set: number[]): PadSpec[] =>
  Array.from({ length: 8 }, (_, i) => ({
    label: String(i + 1),
    ariaLabel: `Hot cue ${i + 1}${set.includes(i) ? "" : " (empty)"}`,
    color: cueToken(i),
    lit: set.includes(i),
  }));

interface DeckSpec {
  index: number;
  name: string;
  title: string;
  deck: MockDeck;
  cues: number[];
}

function DeckColumn({ spec }: { spec: DeckSpec }) {
  const track = storyTrack(SECONDS);
  return (
    <section
      aria-label={`Deck ${spec.name}`}
      className={`flex min-w-0 flex-1 flex-col gap-3 rounded-md border border-border border-t-2 bg-surface p-3 ${DECK_BORDER[spec.index]}`}
    >
      <header className="flex items-baseline justify-between gap-2">
        <span className={`font-mono text-sm font-semibold ${DECK_TEXT[spec.index]}`}>{spec.name}</span>
        <span className="min-w-0 flex-1 truncate text-sm text-fg">{spec.title}</span>
        <span className="font-mono text-sm text-fg-muted">{(MOCK_BPM * spec.deck.rate).toFixed(2)} BPM</span>
      </header>
      <Overview
        label={`Deck ${spec.name} overview`}
        level={track.peaks.overview}
        markers={track.markers}
        frameSource={spec.deck.source}
        onSeek={(s) => spec.deck.seek(s)}
      />
      <div className="flex items-center gap-4">
        <JogWheel
          deck={spec.index}
          label={`Deck ${spec.name} jog wheel`}
          frameSource={spec.deck.source}
          cuePoint={track.markers.hotCues[0]?.start ?? null}
          className="size-36 shrink-0"
          onTouch={() => undefined}
          onTurn={(r) => spec.deck.seek(spec.deck.source(performance.now()).position + r * 1.8)}
          onSearch={() => undefined}
        />
        <div className="min-w-0 flex-1">
          <PerformancePads
            label={`Deck ${spec.name} hot cues`}
            pads={hotCues(spec.cues)}
            columns={4}
            onPress={() => undefined}
            onRelease={() => undefined}
          />
        </div>
      </div>
    </section>
  );
}

function Channel({ name, offset }: { name: string; offset: number }) {
  const [eq, setEq] = useState({ high: 0.5, mid: 0.5, low: 0.5 });
  const [trim, setTrim] = useState(0.5);
  const [volume, setVolume] = useState(0.8);
  const read = useCallback(() => mockMeter(performance.now(), { offset }), [offset]);
  return (
    <div className="flex flex-col items-center gap-2">
      <span className="font-mono text-xs text-fg-muted">{name}</span>
      <div className="w-12">
        <Knob label="Trim" name={`Trim ${name}`} value={trim} defaultValue={0.5} bipolar onChange={setTrim} />
      </div>
      {(["high", "mid", "low"] as const).map((band) => (
        <div key={band} className="w-12">
          <Knob
            label={BAND_LABEL[band]}
            name={`${BAND_LABEL[band]} ${name}`}
            value={eq[band]}
            defaultValue={0.5}
            bipolar
            onChange={(v) => setEq((e) => ({ ...e, [band]: v }))}
          />
        </div>
      ))}
      <div className="mt-2 flex h-36 items-end gap-2">
        <LevelMeterCanvas label={`Level ${name}`} read={read} />
        <Slider
          aria-label={`Volume ${name}`}
          orientation="vertical"
          min={0}
          max={1}
          step={0.01}
          value={[volume]}
          onValueChange={([v]) => setVolume(v ?? 0)}
        />
      </div>
    </div>
  );
}

function Mixer() {
  const [xfade, setXfade] = useState(0.5);
  return (
    <section
      aria-label="Mixer"
      className="flex w-44 shrink-0 flex-col items-center gap-3 rounded-md border border-border bg-surface p-3"
    >
      <div className="flex gap-4">
        <Channel name="A" offset={0} />
        <Channel name="B" offset={0.37} />
      </div>
      <Slider
        aria-label="Crossfader"
        min={0}
        max={1}
        step={0.01}
        value={[xfade]}
        onValueChange={([v]) => setXfade(v ?? 0.5)}
      />
    </section>
  );
}

function TwoDecksAndMixer() {
  const track = storyTrack(SECONDS);
  const [a] = useState(() => storyDeck(SECONDS, { position: 34, rate: 1.02 }));
  const [b] = useState(() => storyDeck(SECONDS, { position: 92, rate: 0.98 }));
  const [zoomA, setZoomA] = useState(2);
  const [zoomB, setZoomB] = useState(2);
  const decks: DeckSpec[] = [
    { index: 0, name: "A", title: "Synthetic track one", deck: a, cues: [0, 1, 2, 4, 7] },
    { index: 1, name: "B", title: "Synthetic track two", deck: b, cues: [0, 3, 5] },
  ];
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1 rounded-md border border-border bg-bg-subtle p-1">
        <Waveform
          label="Deck A waveform"
          levels={track.peaks.detail}
          markers={track.markers}
          frameSource={a.source}
          zoomIndex={zoomA}
          onZoomChange={setZoomA}
          onScrub={(s) => a.seek(s)}
          className="h-28"
        />
        <Waveform
          label="Deck B waveform"
          levels={track.peaks.detail}
          markers={track.markers}
          frameSource={b.source}
          zoomIndex={zoomB}
          onZoomChange={setZoomB}
          onScrub={(s) => b.seek(s)}
          className="h-28"
        />
      </div>
      <div className="flex items-start gap-3">
        {decks.map((spec) => (
          <div key={spec.name} className={`flex min-w-0 flex-1 ${spec.index === 0 ? "order-1" : "order-3"}`}>
            <DeckColumn spec={spec} />
          </div>
        ))}
        <div className="order-2 flex">
          <Mixer />
        </div>
      </div>
    </div>
  );
}

export const TwoDecksMixer: Story = { render: () => <TwoDecksAndMixer /> };
