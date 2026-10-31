import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  IconButton,
  Panel,
  PanelBody,
  PanelHeader,
} from "@opendeck/design-system";
import { X } from "lucide-react";
import { BEAT_FX_UNITS_SHOWN, SEND_BUSES_SHOWN } from "../../lib/fx";
import { useAppStore } from "../../state/app";
import { useFxStore } from "../../state/fx";
import { BeatFxUnit } from "./BeatFxUnit";
import { PadFxEditor } from "./PadFxEditor";
import { SendUnit } from "./SendUnit";

/** FX band: two Beat FX units, the send buses and the pad FX editor. The top bar toggles it. */
export function FxPanel() {
  const setFxOpen = useAppStore((s) => s.setFxOpen);
  const ready = useFxStore((s) => s.state !== null);
  return (
    <Panel flush aria-label="Effects" className="size-full">
      <PanelHeader
        title="Effects"
        actions={
          <div className="flex items-center gap-1">
            <Dialog>
              <DialogTrigger asChild>
                <Button size="sm" variant="ghost" disabled={!ready}>
                  Pad FX
                </Button>
              </DialogTrigger>
              <DialogContent className="max-w-3xl">
                <DialogHeader>
                  <DialogTitle>Pad FX</DialogTitle>
                  <DialogDescription>
                    32 pad effects: two banks, each with a shift layer. Hold a pad to engage,
                    release to stop.
                  </DialogDescription>
                </DialogHeader>
                <PadFxEditor />
              </DialogContent>
            </Dialog>
            <IconButton size="sm" aria-label="Close effects" onClick={() => setFxOpen(false)}>
              <X aria-hidden />
            </IconButton>
          </div>
        }
      />
      <PanelBody className="flex flex-wrap items-start gap-3">
        {ready ? (
          <>
            {Array.from({ length: BEAT_FX_UNITS_SHOWN }, (_, i) => (
              <BeatFxUnit key={i} index={i} />
            ))}
            {Array.from({ length: SEND_BUSES_SHOWN }, (_, i) => (
              <SendUnit key={i} bus={i} />
            ))}
          </>
        ) : (
          <p className="text-sm text-fg-subtle">Effects are loading.</p>
        )}
      </PanelBody>
    </Panel>
  );
}
