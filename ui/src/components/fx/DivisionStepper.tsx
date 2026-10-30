import { IconButton } from "@opendeck/design-system";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { FxUnitRef } from "../../ipc/bindings";
import { formatDivision } from "../../lib/fx";
import { applyFx } from "../../state/fx";

interface DivisionStepperProps {
  unit: FxUnitRef;
  division: number | null;
  /** The effect has no beat sync: the stepper is inert. */
  disabled?: boolean;
  scope: string;
}

/** Beat division readout with BEAT left / right steppers. */
export function DivisionStepper({ unit, division, disabled = false, scope }: DivisionStepperProps) {
  return (
    <div className="flex items-center gap-1" role="group" aria-label={`${scope} beat division`}>
      <IconButton
        size="sm"
        aria-label={`${scope} beat left`}
        disabled={disabled}
        onClick={() => applyFx({ kind: "step_division", unit, direction: -1 })}
      >
        <ChevronLeft aria-hidden />
      </IconButton>
      <output
        aria-label={`${scope} division`}
        className="min-w-9 text-center font-mono text-xs tabular-nums text-fg"
      >
        {disabled ? "-" : formatDivision(division)}
      </output>
      <IconButton
        size="sm"
        aria-label={`${scope} beat right`}
        disabled={disabled}
        onClick={() => applyFx({ kind: "step_division", unit, direction: 1 })}
      >
        <ChevronRight aria-hidden />
      </IconButton>
    </div>
  );
}
