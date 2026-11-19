import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@opendeck/design-system";
import { Layers } from "lucide-react";
import { BeatFxCompact } from "./BeatFxCompact";
import { FxPanel } from "./FxPanel";

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
 * Beat FX bar under the menu row. It uses the deck row's columns: Beat FX 1
 * above deck 1, All FX above the mixer, Beat FX 2 above deck 2. The FX toggle
 * in the menu row hides it.
 */
export function FxBar() {
  return (
    <div className="grid h-[var(--fx-bar-h)] shrink-0 grid-cols-[minmax(0,1fr)_var(--mixer-w)_minmax(0,1fr)] gap-px border-b border-border bg-border">
      <section aria-label="Beat FX 1" className="min-w-0 bg-bg-subtle">
        <BeatFxCompact index={0} />
      </section>
      <div className="flex items-center justify-center bg-bg-subtle">
        <AllEffects />
      </div>
      <section aria-label="Beat FX 2" className="min-w-0 bg-bg-subtle">
        <BeatFxCompact index={1} />
      </section>
    </div>
  );
}
