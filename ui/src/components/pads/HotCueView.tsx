import { Button, IconButton, Input } from "@opendeck/design-system";
import { Play, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { commands } from "../../ipc/bindings";
import { formatTime } from "../../lib/format";
import { hotCuePress, hotCueRelease, report } from "../../state/actions";
import { HOT_CUE_COUNT, selectDeckUi, useAppStore } from "../../state/app";
import { renameHotCue } from "../../state/pads";
import { cueToken } from "../../viz/colors";
import { useDeckLoaded } from "./useDeckPads";

/** Name field that commits on blur or Enter, so typing does not hit the backend per key. */
function CueName({ deck, index, name }: { deck: number; index: number; name: string }) {
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft !== null && draft !== name) {
      renameHotCue(deck, index, draft);
    }
    setDraft(null);
  };
  return (
    <Input
      aria-label={`Hot cue ${index + 1} name`}
      placeholder="Name"
      className="h-7 min-w-0 flex-1"
      value={draft ?? name}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.currentTarget.blur();
        } else if (event.key === "Escape") {
          setDraft(null);
        }
      }}
    />
  );
}

/** Eight hot cue rows: colour, name, time, set, jump and delete. */
export function HotCueView({ deck }: { deck: number }) {
  const { hotCues } = useAppStore(selectDeckUi(deck));
  const loaded = useDeckLoaded(deck);
  return (
    <ul aria-label={`Deck ${deck + 1} hot cues`} className="flex flex-col gap-1">
      {Array.from({ length: HOT_CUE_COUNT }, (_, index) => {
        const cue = hotCues[index] ?? null;
        return (
          <li key={index} className="flex items-center gap-2">
            <span
              aria-hidden
              className="size-3 shrink-0 rounded-sm"
              style={{
                backgroundColor: cue ? `var(--${cueToken(index)})` : "var(--border)",
              }}
            />
            <span className="w-4 shrink-0 text-xs tabular-nums text-fg-muted">{index + 1}</span>
            {cue ? (
              <CueName deck={deck} index={index} name={cue.name} />
            ) : (
              <span className="min-w-0 flex-1 truncate text-sm text-fg-subtle">Empty</span>
            )}
            <span className="w-14 shrink-0 text-right font-mono text-xs text-fg-muted">
              {cue ? formatTime(cue.start ?? 0) : "--:--"}
            </span>
            <IconButton
              size="sm"
              aria-label={`Set hot cue ${index + 1} at the playhead`}
              disabled={!loaded}
              onClick={() => void report(commands.setHotCue(deck, index))}
            >
              <Plus className="size-4" aria-hidden />
            </IconButton>
            <Button
              size="sm"
              aria-label={`Jump to hot cue ${index + 1}`}
              disabled={!cue}
              onPointerDown={() => hotCuePress(deck, index)}
              onPointerUp={() => hotCueRelease(deck, index)}
              onPointerLeave={() => hotCueRelease(deck, index)}
            >
              <Play className="size-3" aria-hidden />
            </Button>
            <IconButton
              size="sm"
              aria-label={`Delete hot cue ${index + 1}`}
              disabled={!cue}
              onClick={() => void report(commands.deleteHotCue(deck, index))}
            >
              <Trash2 className="size-4" aria-hidden />
            </IconButton>
          </li>
        );
      })}
    </ul>
  );
}
