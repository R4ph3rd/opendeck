import {
  Badge,
  Button,
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  IconButton,
  Toggle,
  cn,
} from "@opendeck/design-system";
import { Activity, AudioLines, ChevronDown, CircleHelp, Piano, Settings } from "lucide-react";
import { useEffect, useState, type ComponentProps } from "react";
import { useAppStore } from "../state/app";
import { useMidiStore } from "../state/midi";
import { DEFAULT_LAYOUT, usePrefsStore } from "../state/prefs";
import { ImportMenu } from "./ImportMenu";

const KHZ = 1000;
/** Clock refresh, ms. The clock shows minutes only. */
const CLOCK_TICK_MS = 15_000;

/** Flat toolbar button: uppercase label, no icon, like the menu row of a DJ application. */
const MENU_BUTTON = "h-5 rounded-sm bg-surface-raised px-2 text-xs font-bold uppercase";

function MenuTrigger({ className, children, ...props }: ComponentProps<typeof Button>) {
  return (
    <Button
      size="sm"
      variant="ghost"
      className={cn(MENU_BUTTON, "gap-1 hover:bg-surface-hover", className)}
      {...props}
    >
      {children}
      <ChevronDown className="size-3" aria-hidden />
    </Button>
  );
}

/** Wall clock, `HH:MM`. */
function Clock() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), CLOCK_TICK_MS);
    return () => clearInterval(id);
  }, []);
  return (
    <time
      dateTime={now.toISOString()}
      className="font-mono text-xs tabular-nums text-fg-muted"
      aria-label="Clock"
    >
      {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
    </time>
  );
}

/** Mode selector. Performance is the only mode today. */
function ModeMenu() {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <MenuTrigger aria-label="Mode: Performance">Performance</MenuTrigger>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuLabel>Mode</DropdownMenuLabel>
        <DropdownMenuRadioGroup value="performance">
          <DropdownMenuRadioItem value="performance">Performance</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Which panels of the layout are shown, and layout reset. */
function ViewMenu() {
  const { wavesOpen, fxOpen, samplerOpen, browserOpen } = useAppStore();
  const setWavesOpen = useAppStore((s) => s.setWavesOpen);
  const setFxOpen = useAppStore((s) => s.setFxOpen);
  const setSamplerOpen = useAppStore((s) => s.setSamplerOpen);
  const setBrowserOpen = useAppStore((s) => s.setBrowserOpen);
  const restore = usePrefsStore((s) => s.restoreLayout);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <MenuTrigger>View</MenuTrigger>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuLabel>Show</DropdownMenuLabel>
        <DropdownMenuCheckboxItem checked={wavesOpen} onCheckedChange={setWavesOpen}>
          Waveforms
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={fxOpen} onCheckedChange={setFxOpen}>
          Effects
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={samplerOpen} onCheckedChange={setSamplerOpen}>
          Sampler
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem checked={browserOpen} onCheckedChange={setBrowserOpen}>
          Browser
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => restore(DEFAULT_LAYOUT)}>Reset layout</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Layout selector: the 2-deck layout, saved layouts and the layout settings. */
function LayoutMenu() {
  const presets = usePrefsStore((s) => s.layoutPresets);
  const restore = usePrefsStore((s) => s.restoreLayout);
  const openSettings = useAppStore((s) => s.openSettings);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <MenuTrigger aria-label="Layout: 2 Deck Horizontal" className="normal-case">
          2 Deck Horizontal
        </MenuTrigger>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Layout</DropdownMenuLabel>
        <DropdownMenuRadioGroup value="2deck">
          <DropdownMenuRadioItem value="2deck">2 Deck Horizontal</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="4deck" disabled>
            4 Deck Horizontal (planned)
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
        {presets.length > 0 ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Saved layouts</DropdownMenuLabel>
            {presets.map((preset) => (
              <DropdownMenuItem key={preset.name} onSelect={() => restore(preset.layout)}>
                {preset.name}
              </DropdownMenuItem>
            ))}
          </>
        ) : null}
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={() => openSettings("interface")}>
          Layout settings...
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/**
 * Menu row of the performance layout. Left: mode, FX, SAMPLER, REC, VIEW and
 * import. Centre: the OpenDeck wordmark. Right: layout selector, controller
 * and audio status, clock, help and settings.
 */
export function TopBar() {
  const audio = useAppStore((s) => s.audio);
  const setSettingsOpen = useAppStore((s) => s.setSettingsOpen);
  const openSettings = useAppStore((s) => s.openSettings);
  const setMonitorOpen = useAppStore((s) => s.setMidiMonitorOpen);
  const midi = useMidiStore((s) => s.status);
  const openFirstRun = usePrefsStore((s) => s.openFirstRun);
  const fxOpen = useAppStore((s) => s.fxOpen);
  const setFxOpen = useAppStore((s) => s.setFxOpen);
  const samplerOpen = useAppStore((s) => s.samplerOpen);
  const setSamplerOpen = useAppStore((s) => s.setSamplerOpen);

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
    <header className="relative flex h-[var(--toolbar-h)] shrink-0 items-center gap-1 border-b border-border bg-toolbar px-2">
      <ModeMenu />
      <Toggle
        size="sm"
        aria-label="Show effects"
        pressed={fxOpen}
        onPressedChange={setFxOpen}
        className={cn(MENU_BUTTON, "data-[state=on]:bg-accent data-[state=on]:text-accent-fg")}
      >
        FX
      </Toggle>
      <Toggle
        size="sm"
        aria-label="Show sampler"
        pressed={samplerOpen}
        onPressedChange={setSamplerOpen}
        className={cn(MENU_BUTTON, "data-[state=on]:bg-accent data-[state=on]:text-accent-fg")}
      >
        Sampler
      </Toggle>
      <Button
        size="sm"
        variant="ghost"
        disabled
        title="Recording is not available yet"
        className={MENU_BUTTON}
      >
        Rec
      </Button>
      <ViewMenu />
      <ImportMenu />

      <h1 className="pointer-events-none absolute inset-x-0 text-center text-sm font-semibold tracking-wider text-fg">
        OpenDeck
      </h1>

      <div className="flex-1" />
      <LayoutMenu />
      <button
        type="button"
        onClick={() => openSettings("midi")}
        aria-label={`MIDI controller: ${midiText}. Open MIDI settings`}
        className="rounded-sm focus-visible:outline-2 focus-visible:outline-focus-ring"
      >
        <Badge variant={midiVariant} className="max-w-40 truncate">
          <Piano className="size-3 shrink-0" aria-hidden />
          <span className="truncate">{midiText}</span>
        </Badge>
      </button>
      <IconButton size="sm" aria-label="MIDI monitor" onClick={() => setMonitorOpen(true)}>
        <Activity className="size-4" aria-hidden />
      </IconButton>
      <button
        type="button"
        onClick={() => setSettingsOpen(true)}
        aria-label={`Audio output: ${status}. Open settings`}
        className="rounded-sm focus-visible:outline-2 focus-visible:outline-focus-ring"
      >
        <Badge variant={variant} className="max-w-48 truncate">
          <AudioLines className="size-3 shrink-0" aria-hidden />
          <span className="truncate">{status}</span>
        </Badge>
      </button>
      <Clock />
      <IconButton size="sm" aria-label="Setup guide" onClick={openFirstRun}>
        <CircleHelp className="size-4" aria-hidden />
      </IconButton>
      <IconButton size="sm" aria-label="Settings" onClick={() => setSettingsOpen(true)}>
        <Settings className="size-4" aria-hidden />
      </IconButton>
    </header>
  );
}
