import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Panel,
} from "@opendeck/design-system";
import { Layers } from "lucide-react";
import { useAppStore } from "../state/app";
import { BeatFxCompact } from "./fx/BeatFxCompact";
import { FxPanel } from "./fx/FxPanel";
import { PadMiddlePanel } from "./pads/PadMiddlePanel";

/** Dialog with every effect control: both units in full, the send buses and Pad FX. */
function AllEffects() {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button size="sm" variant="ghost" aria-label="All effects, sends and Pad FX">
          <Layers aria-hidden />
          All FX
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-5xl">
        <DialogHeader>
          <DialogTitle>Effects</DialogTitle>
          <DialogDescription>Beat FX units, send buses and Pad FX.</DialogDescription>
        </DialogHeader>
        <div className="h-96">
          <FxPanel />
        </div>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Middle row, between the decks and the browser: Beat FX 1 on the left, the
 * pad-mode or sampler panes in the centre, Beat FX 2 on the right. The FX
 * toolbar toggle hides both effect blocks.
 */
export function MiddleRow() {
  const fxOpen = useAppStore((s) => s.fxOpen);
  return (
    <div className="flex size-full min-h-0 gap-px bg-border">
      {fxOpen ? (
        <Panel
          flush
          aria-label="Beat FX 1"
          className="w-[var(--fx-unit-w)] shrink-0 bg-bg-subtle"
        >
          <BeatFxCompact index={0} />
        </Panel>
      ) : null}
      <div className="min-w-0 flex-1">
        <PadMiddlePanel />
      </div>
      {fxOpen ? (
        <Panel
          flush
          aria-label="Beat FX 2"
          className="w-[var(--fx-unit-w)] shrink-0 bg-bg-subtle"
        >
          <div className="min-h-0 flex-1">
            <BeatFxCompact index={1} />
          </div>
          <div className="flex shrink-0 justify-end px-2 pb-1">
            <AllEffects />
          </div>
        </Panel>
      ) : null}
    </div>
  );
}
