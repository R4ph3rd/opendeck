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
import { ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";
import { BEAT_FX_UNITS_SHOWN, SEND_BUSES_SHOWN } from "../../lib/fx";
import { useFxStore } from "../../state/fx";
import { BeatFxUnit } from "./BeatFxUnit";
import { PadFxEditor } from "./PadFxEditor";
import { SendUnit } from "./SendUnit";

/** Collapsible FX strip: two Beat FX units, the send buses and the pad FX editor. */
export function FxPanel() {
  const [open, setOpen] = useState(true);
  const ready = useFxStore((s) => s.state !== null);
  return (
    <Panel aria-label="Effects" className="shrink-0">
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
                    32 pad effects: two banks, each with a shift layer. Hold a pad to engage, release
                    to stop.
                  </DialogDescription>
                </DialogHeader>
                <PadFxEditor />
              </DialogContent>
            </Dialog>
            <IconButton
              size="sm"
              aria-label={open ? "Collapse effects" : "Expand effects"}
              aria-expanded={open}
              onClick={() => setOpen((v) => !v)}
            >
              {open ? <ChevronUp aria-hidden /> : <ChevronDown aria-hidden />}
            </IconButton>
          </div>
        }
      />
      {open ? (
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
      ) : null}
    </Panel>
  );
}
