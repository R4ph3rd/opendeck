import { Button, Slider, Toggle, ToggleGroup, ToggleGroupItem } from "@opendeck/design-system";
import type { TempoRange } from "../ipc/bindings";
import { formatTempo } from "../lib/format";
import { pitchBend, setKeyLock, setTempoFader, setTempoRange } from "../state/actions";
import {
  TEMPO_RANGE_FRACTION,
  TEMPO_RANGE_LABEL,
  TEMPO_RANGES,
  selectDeckUi,
  useAppStore,
} from "../state/app";

/** Slider resolution: fine enough for 0.1% steps on the narrowest range. */
const FADER_STEP = 0.001;

interface TempoControlsProps {
  deck: number;
  disabled: boolean;
}

/** Tempo fader, range selector, key lock and pitch bend for one deck. */
export function TempoControls({ deck, disabled }: TempoControlsProps) {
  const { tempoFader, tempoRange, keyLock } = useAppStore(selectDeckUi(deck));
  const bendProps = (amount: number) => ({
    onPointerDown: (event: React.PointerEvent<HTMLButtonElement>) => {
      event.currentTarget.setPointerCapture?.(event.pointerId);
      pitchBend(deck, amount);
    },
    onPointerUp: () => pitchBend(deck, 0),
    onPointerCancel: () => pitchBend(deck, 0),
  });

  return (
    <div className="flex w-28 shrink-0 flex-col items-center gap-2" aria-label="Tempo">
      <output
        aria-label="Tempo change"
        className="font-mono text-base tabular-nums text-fg"
        data-testid={`tempo-readout-${deck}`}
      >
        {formatTempo(tempoFader, TEMPO_RANGE_FRACTION[tempoRange])}
      </output>
      {/* The design-system Slider does not forward a name to its thumb, so name the group. */}
      <div role="group" aria-label="Tempo fader">
        <Slider
          orientation="vertical"
          className="h-32"
          min={-1}
          max={1}
          step={FADER_STEP}
          value={[tempoFader]}
          disabled={disabled}
          onValueChange={([value]) => setTempoFader(deck, value ?? 0)}
          onDoubleClick={() => setTempoFader(deck, 0)}
        />
      </div>
      <ToggleGroup
        type="single"
        className="grid grid-cols-2"
        aria-label="Tempo range"
        value={tempoRange}
        disabled={disabled}
        onValueChange={(value) => {
          if (value) {
            setTempoRange(deck, value as TempoRange);
          }
        }}
      >
        {TEMPO_RANGES.map((range) => (
          <ToggleGroupItem key={range} value={range} size="sm" aria-label={`Range ${TEMPO_RANGE_LABEL[range]}`}>
            {TEMPO_RANGE_LABEL[range]}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <Toggle
        variant="outline"
        size="sm"
        pressed={keyLock}
        disabled={disabled}
        onPressedChange={(on) => setKeyLock(deck, on)}
      >
        KEY LOCK
      </Toggle>
      <div className="flex gap-1">
        <Button size="sm" disabled={disabled} aria-label="Pitch bend down" {...bendProps(-1)}>
          -
        </Button>
        <Button size="sm" disabled={disabled} aria-label="Pitch bend up" {...bendProps(1)}>
          +
        </Button>
      </div>
    </div>
  );
}
