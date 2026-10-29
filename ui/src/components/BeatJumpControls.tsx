import {
  Button,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@opendeck/design-system";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { beatJump, setJumpBeats } from "../state/actions";
import { JUMP_BEATS, selectDeckUi, useAppStore } from "../state/app";
import { beatLabel } from "./LoopControls";

interface BeatJumpControlsProps {
  deck: number;
  disabled: boolean;
}

/** Beat jump left and right with a selectable size. */
export function BeatJumpControls({ deck, disabled }: BeatJumpControlsProps) {
  const { jumpBeats } = useAppStore(selectDeckUi(deck));

  return (
    <div className="flex items-center gap-2" role="group" aria-label="Beat jump">
      <Button
        size="sm"
        aria-label="Beat jump back"
        disabled={disabled}
        onClick={() => beatJump(deck, -1)}
      >
        <ChevronLeft className="size-4" aria-hidden />
      </Button>
      <Select
        value={String(jumpBeats)}
        disabled={disabled}
        onValueChange={(value) => setJumpBeats(deck, Number(value))}
      >
        <SelectTrigger className="w-20" aria-label="Beat jump size">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {JUMP_BEATS.map((beats) => (
            <SelectItem key={beats} value={String(beats)}>
              {beatLabel(beats)} beats
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button
        size="sm"
        aria-label="Beat jump forward"
        disabled={disabled}
        onClick={() => beatJump(deck, 1)}
      >
        <ChevronRight className="size-4" aria-hidden />
      </Button>
    </div>
  );
}
