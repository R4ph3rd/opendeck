import { Badge, IconButton, Toggle } from "@opendeck/design-system";
import { Activity, AudioLines, CircleHelp, Piano, Settings } from "lucide-react";
import { useAppStore } from "../state/app";
import { useMidiStore } from "../state/midi";
import { usePrefsStore } from "../state/prefs";
import { ImportMenu } from "./ImportMenu";

const KHZ = 1000;

/** App title, audio output status and the settings entry. */
export function TopBar() {
  const audio = useAppStore((s) => s.audio);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const openSettings = useAppStore((s) => s.openSettings);
  const setMonitorOpen = useAppStore((s) => s.setMidiMonitorOpen);
  const midi = useMidiStore((s) => s.status);
  const openFirstRun = usePrefsStore((s) => s.openFirstRun);
  const fxOpen = useAppStore((s) => s.fxOpen);
  const setFxOpen = useAppStore((s) => s.setFxOpen);

  let status = "Audio starting";
  let variant: "success" | "danger" | "neutral" = "neutral";
  if (audio?.running) {
    const rate = audio.sample_rate ? ` ${audio.sample_rate / KHZ} kHz` : "";
    status = `${audio.device ?? "Output"}${rate}`;
    variant = "success";
  } else if (audio) {
    status = "Audio off";
    variant = "danger";
  }

  let midiText = "No controller";
  let midiVariant: "success" | "neutral" | "danger" = "neutral";
  if (midi?.connected) {
    midiText = midi.input ?? midi.mapping_name;
    midiVariant = "success";
  } else if (midi && !midi.enabled) {
    midiText = "MIDI off";
    midiVariant = "danger";
  }

  return (
    <header className="flex h-9 shrink-0 items-center gap-3 border-b border-border bg-toolbar px-3">
      <h1 className="text-base font-semibold tracking-wide text-fg">OpenDeck</h1>
      <div className="flex-1" />
      <Toggle
        size="sm"
        variant="outline"
        aria-label="Show effects"
        pressed={fxOpen}
        onPressedChange={setFxOpen}
      >
        FX
      </Toggle>
      <ImportMenu />
      <button
        type="button"
        onClick={() => openSettings("midi")}
        aria-label={`MIDI controller: ${midiText}. Open MIDI settings`}
        className="rounded-sm focus-visible:outline-2 focus-visible:outline-focus-ring"
      >
        <Badge variant={midiVariant} className="max-w-48 truncate">
          <Piano className="size-3 shrink-0" aria-hidden />
          <span className="truncate">{midiText}</span>
        </Badge>
      </button>
      <IconButton aria-label="MIDI monitor" onClick={() => setMonitorOpen(true)}>
        <Activity className="size-4" aria-hidden />
      </IconButton>
      <button
        type="button"
        onClick={() => setSettingsOpen(true)}
        aria-label={`Audio output: ${status}. Open settings`}
        className="rounded-sm focus-visible:outline-2 focus-visible:outline-focus-ring"
      >
        <Badge variant={variant} className="max-w-64 truncate">
          <AudioLines className="size-3 shrink-0" aria-hidden />
          <span className="truncate">{status}</span>
        </Badge>
      </button>
      <IconButton aria-label="Setup guide" onClick={openFirstRun}>
        <CircleHelp className="size-4" aria-hidden />
      </IconButton>
      <IconButton aria-label="Settings" onClick={() => setSettingsOpen(true)}>
        <Settings className="size-4" aria-hidden />
      </IconButton>
    </header>
  );
}
