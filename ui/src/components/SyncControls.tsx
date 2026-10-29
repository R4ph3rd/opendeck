import { Toggle, ToggleGroup, ToggleGroupItem } from "@opendeck/design-system";
import { useStore } from "zustand";
import type { SyncMode } from "../ipc/bindings";
import { setSync, toggleLeader, toggleQuantize, toggleSlip, toggleSync } from "../state/actions";
import { selectDeckUi, useAppStore } from "../state/app";
import { selectLeader, telemetryStore } from "../state/telemetry";

interface SyncControlsProps {
  deck: number;
  disabled: boolean;
}

const MODES: { value: Exclude<SyncMode, "off">; label: string }[] = [
  { value: "tempo", label: "Tempo" },
  { value: "beat", label: "Beat" },
];

/** SYNC with its mode, MASTER (sync leader), QUANTIZE and SLIP. */
export function SyncControls({ deck, disabled }: SyncControlsProps) {
  const { sync, syncPref, quantize, slip } = useAppStore(selectDeckUi(deck));
  const requestedLeader = useAppStore((s) => s.leader);
  const reportedLeader = useStore(telemetryStore, selectLeader);
  // The engine's report wins once it names a leader; until then show the request.
  const isLeader = (reportedLeader ?? requestedLeader) === deck;
  const syncOn = sync !== "off";

  return (
    <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Sync">
      <Toggle
        variant="outline"
        size="sm"
        aria-label="Sync"
        pressed={syncOn}
        disabled={disabled}
        onPressedChange={() => toggleSync(deck)}
        className="data-[state=on]:bg-sync-on data-[state=on]:text-cue-fg"
      >
        SYNC
      </Toggle>
      <ToggleGroup
        type="single"
        aria-label="Sync mode"
        value={syncPref}
        disabled={disabled}
        onValueChange={(value) => {
          if (value) {
            setSync(deck, value as SyncMode);
          }
        }}
      >
        {MODES.map((mode) => (
          <ToggleGroupItem
            key={mode.value}
            value={mode.value}
            size="sm"
            aria-label={`Sync ${mode.label}`}
          >
            {mode.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <Toggle
        variant="outline"
        size="sm"
        aria-label="Master"
        pressed={isLeader}
        disabled={disabled}
        onPressedChange={() => toggleLeader(deck, isLeader)}
        className="data-[state=on]:bg-master-on data-[state=on]:text-cue-fg"
      >
        MASTER
      </Toggle>
      <Toggle
        variant="outline"
        size="sm"
        aria-label="Quantize"
        pressed={quantize}
        disabled={disabled}
        onPressedChange={() => toggleQuantize(deck)}
      >
        QUANT
      </Toggle>
      <Toggle
        variant="outline"
        size="sm"
        aria-label="Slip"
        pressed={slip}
        disabled={disabled}
        onPressedChange={() => toggleSlip(deck)}
      >
        SLIP
      </Toggle>
    </div>
  );
}
