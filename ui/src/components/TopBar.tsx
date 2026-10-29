import { Badge, IconButton } from "@opendeck/design-system";
import { AudioLines, Settings } from "lucide-react";
import { useAppStore } from "../state/app";

const KHZ = 1000;

/** App title, audio output status and the settings entry. */
export function TopBar() {
  const audio = useAppStore((s) => s.audio);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);

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

  return (
    <header className="flex h-10 shrink-0 items-center gap-3 border-b border-border bg-toolbar px-3">
      <h1 className="text-base font-semibold tracking-wide text-fg">OpenDeck</h1>
      <div className="flex-1" />
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
      <IconButton aria-label="Settings" onClick={() => setSettingsOpen(true)}>
        <Settings className="size-4" aria-hidden />
      </IconButton>
    </header>
  );
}
