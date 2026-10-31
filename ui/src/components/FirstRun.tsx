import {
  Badge,
  Button,
  Callout,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  ProgressBar,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@opendeck/design-system";
import { open as pickPath } from "@tauri-apps/plugin-dialog";
import { Check, FolderPlus, Volume2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  commands,
  type AudioDeviceInfo,
  type AudioSettings,
  type TestToneOutput,
} from "../ipc/bindings";
import { pairLabel } from "../lib/audioOptions";
import { errorMessage } from "../lib/errors";
import {
  FIRST_RUN_STEPS,
  FLX4_HEADPHONES,
  FLX4_MASTER,
  findFlx4Device,
  findFlx4Port,
  flx4Settings,
  sawPlayPress,
  type FirstRunStep,
} from "../lib/firstRun";
import { report } from "../state/actions";
import { useAppStore } from "../state/app";
import { useLibraryStore } from "../state/library";
import { importFolder, pickAndImportRekordbox } from "../state/libraryActions";
import { connectMidi, refreshPorts, setMonitor, useMidiStore } from "../state/midi";
import { notifyError, notifySuccess } from "../state/notify";
import { usePrefsStore } from "../state/prefs";

const DECK_ONE = 0;

/** Panels shown on the last step. */
export const TOUR: readonly { name: string; text: string }[] = [
  { name: "Waveforms", text: "The top strip shows both decks. Scroll to zoom, click to seek." },
  { name: "Decks", text: "Transport, tempo, sync, loops and the jog ring for each deck." },
  { name: "Mixer", text: "Trim, three-band EQ, filter, faders, crossfader and headphone cue." },
  { name: "Effects", text: "Beat FX units, Sound Color FX and send effects." },
  { name: "Pads", text: "The middle panel follows each deck's pad mode: hot cues, loops, sampler and more." },
  { name: "Collection", text: "Your tracks, playlists and saved sets. Drag a track onto a deck to load it." },
];

/** First-run guide: audio, controller, library, then a short tour. */
export function FirstRunDialog() {
  const open = usePrefsStore((s) => s.firstRunOpen);
  const complete = usePrefsStore((s) => s.completeFirstRun);
  return (
    <Dialog open={open} onOpenChange={(next) => !next && complete()}>
      <DialogContent className="flex max-h-[85vh] flex-col gap-4 sm:max-w-2xl">
        <FirstRunBody onDone={complete} />
      </DialogContent>
    </Dialog>
  );
}

/** The steps. Mounted only while the dialog is open, so it starts at the first step each time. */
function FirstRunBody({ onDone }: { onDone: () => void }) {
  const [index, setIndex] = useState(0);
  const step: FirstRunStep = FIRST_RUN_STEPS[index] ?? "welcome";
  const last = index === FIRST_RUN_STEPS.length - 1;
  return (
    <>
      <DialogHeader>
        <DialogTitle>Set up OpenDeck</DialogTitle>
        <DialogDescription>
          Step {index + 1} of {FIRST_RUN_STEPS.length}. You can reopen this from the help button.
        </DialogDescription>
      </DialogHeader>
      <ProgressBar aria-label="Setup progress" value={index + 1} max={FIRST_RUN_STEPS.length} />
      <div className="min-h-0 flex-1 overflow-auto">
        {step === "welcome" ? <WelcomeStep /> : null}
        {step === "audio" ? <AudioStep /> : null}
        {step === "controller" ? <ControllerStep /> : null}
        {step === "library" ? <LibraryStep /> : null}
        {step === "tour" ? <TourStep /> : null}
      </div>
      <DialogFooter>
        <Button variant="ghost" onClick={onDone}>
          Skip setup
        </Button>
        {index > 0 ? <Button onClick={() => setIndex(index - 1)}>Back</Button> : null}
        {last ? (
          <Button variant="primary" onClick={onDone}>
            Finish
          </Button>
        ) : (
          <Button variant="primary" onClick={() => setIndex(index + 1)}>
            Next
          </Button>
        )}
      </DialogFooter>
    </>
  );
}

export function WelcomeStep() {
  return (
    <div className="flex flex-col gap-2 text-sm">
      <p>
        OpenDeck is a desktop DJ app. This takes a minute: check the audio output, check your
        controller, then bring in your music.
      </p>
      <p className="text-fg-muted">Every step can be skipped and changed later in Settings.</p>
    </div>
  );
}

/** Output check: pick the device (the FLX4 is preselected), play a tone per output. */
export function AudioStep() {
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [draft, setDraft] = useState<AudioSettings | null>(null);
  const [applied, setApplied] = useState<AudioSettings | null>(null);
  const [applying, setApplying] = useState(false);
  const [tone, setTone] = useState<TestToneOutput | null>(null);

  useEffect(() => {
    let live = true;
    void (async () => {
      const [list, settings] = await Promise.all([
        report(commands.listAudioDevices(null)),
        report(commands.getAudioSettings()),
      ]);
      if (!live || !list || !settings) return;
      setDevices(list);
      setApplied(settings);
      const flx4 = findFlx4Device(list);
      setDraft(flx4 && settings.device === null ? flx4Settings(settings, flx4) : settings);
    })();
    return () => {
      live = false;
    };
  }, []);

  const flx4 = useMemo(() => findFlx4Device(devices), [devices]);

  if (!draft) {
    return <p className="text-sm text-fg-muted">Looking for audio outputs...</p>;
  }

  /** Applies the draft. Returns false when the output could not start. */
  const apply = async (): Promise<boolean> => {
    setApplying(true);
    const status = await report(commands.setAudioSettings(draft));
    setApplying(false);
    if (!status) return false;
    useAppStore.getState().setAudio(status);
    setApplied(draft);
    return true;
  };

  /** The tone plays through the engine, so the draft is applied first when it changed. */
  const play = async (output: TestToneOutput) => {
    setTone(output);
    try {
      const current = JSON.stringify(applied) === JSON.stringify(draft);
      if (!current && !(await apply())) return;
      const result = await commands.playTestTone(output);
      if (result.status === "error") notifyError(errorMessage(result.error));
    } catch (error) {
      notifyError(errorMessage(error));
    } finally {
      setTone(null);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      {flx4 ? (
        <Callout variant="success" title="DDJ-FLX4 found">
          Master is on outputs {pairLabel(FLX4_MASTER)} and headphones on {pairLabel(FLX4_HEADPHONES)}.
        </Callout>
      ) : (
        <Callout variant="info" title="No DDJ-FLX4 audio output found">
          Pick the output you want to use. You can connect the controller later.
        </Callout>
      )}
      <label className="flex flex-col gap-1 text-xs font-medium text-fg-muted">
        Output device
        <Select
          value={draft.device ?? "default"}
          onValueChange={(v) => setDraft({ ...draft, device: v === "default" ? null : v })}
        >
          <SelectTrigger aria-label="Output device">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="default">Default device</SelectItem>
            {devices.map((d) => (
              <SelectItem key={d.name} value={d.name}>
                {d.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <div className="flex flex-wrap gap-2">
        <Button
          loading={tone === "master"}
          onClick={() => void play("master")}
          aria-label={`Play test tone on master ${pairLabel(draft.master)}`}
        >
          <Volume2 className="size-3.5" aria-hidden />
          Test master {pairLabel(draft.master)}
        </Button>
        <Button
          loading={tone === "headphones"}
          disabled={!draft.headphones}
          onClick={() => void play("headphones")}
          aria-label={`Play test tone on headphones ${draft.headphones ? pairLabel(draft.headphones) : ""}`}
        >
          <Volume2 className="size-3.5" aria-hidden />
          Test headphones {draft.headphones ? pairLabel(draft.headphones) : ""}
        </Button>
        <Button
          variant="primary"
          loading={applying}
          onClick={() => void apply().then((ok) => ok && notifySuccess("Audio output set."))}
        >
          Use this output
        </Button>
      </div>
      <p className="text-xs text-fg-muted">
        The tone is quiet and lasts about a second. Testing applies the chosen output first.
      </p>
    </div>
  );
}

/** Controller check: FLX4 detected by the MIDI service, live PLAY press on deck 1. */
export function ControllerStep() {
  const status = useMidiStore((s) => s.status);
  const ports = useMidiStore((s) => s.ports);
  const monitor = useMidiStore((s) => s.monitor);
  const port = findFlx4Port(ports);
  const connected = Boolean(status?.connected && status.input);
  const pressed = sawPlayPress(monitor, DECK_ONE);

  useEffect(() => {
    void refreshPorts();
    void setMonitor(true);
    return () => void setMonitor(false);
  }, []);

  return (
    <div className="flex flex-col gap-3 text-sm">
      <div className="flex items-center gap-2">
        <Badge variant={connected ? "success" : "neutral"}>
          {connected ? `Connected: ${status?.input}` : port ? `Found: ${port.name}` : "No controller"}
        </Badge>
        {!connected && port ? (
          <Button size="sm" onClick={() => void connectMidi(port.name)}>
            Connect
          </Button>
        ) : null}
        <Button size="sm" variant="ghost" onClick={() => void refreshPorts()}>
          Rescan
        </Button>
      </div>
      {!connected && !port ? (
        <p className="text-fg-muted">
          Plug in the DDJ-FLX4 and switch it on. No controller? Skip this step and use the
          keyboard.
        </p>
      ) : null}
      <div
        role="status"
        className="flex items-center gap-2 rounded-md border border-border p-3"
        data-testid="play-check"
      >
        {pressed ? (
          <>
            <Check className="size-4 text-success" aria-hidden />
            <span>PLAY on deck 1 received. The controller works.</span>
          </>
        ) : (
          <span className="text-fg-muted">Press PLAY on deck 1 to check the connection.</span>
        )}
      </div>
    </div>
  );
}

/** Library step: choose folders to import, or import a rekordbox XML export. */
export function LibraryStep() {
  const folders = usePrefsStore((s) => s.library.folders);
  const setLibrary = usePrefsStore((s) => s.setLibrary);
  const jobs = useLibraryStore((s) => s.jobs);
  const running = jobs.filter((j) => j.state === "running" || j.state === "queued").length;

  const addFolder = async () => {
    try {
      const picked = await pickPath({ directory: true, multiple: false, title: "Choose music folder" });
      if (typeof picked !== "string") return;
      if (!folders.includes(picked)) setLibrary({ folders: [...folders, picked] });
      await importFolder(picked);
    } catch (error) {
      notifyError(errorMessage(error));
    }
  };

  return (
    <div className="flex flex-col gap-3 text-sm">
      <p>Choose the folders that hold your music. Importing starts at once and runs in the background.</p>
      {folders.length > 0 ? (
        <ul className="flex flex-col gap-1" aria-label="Music folders">
          {folders.map((f) => (
            <li key={f} className="truncate font-mono text-xs" title={f}>
              {f}
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex flex-wrap gap-2">
        <Button variant="primary" onClick={() => void addFolder()}>
          <FolderPlus className="size-3.5" aria-hidden />
          Add music folder
        </Button>
        <Button onClick={() => void pickAndImportRekordbox()}>Import rekordbox XML</Button>
      </div>
      {running > 0 ? (
        <p role="status" className="text-fg-muted">
          {running === 1 ? "1 import is running." : `${running} imports are running.`}
        </p>
      ) : null}
    </div>
  );
}

export function TourStep() {
  return (
    <ul className="flex flex-col gap-2 text-sm" aria-label="Panels">
      {TOUR.map((panel) => (
        <li key={panel.name}>
          <span className="font-semibold">{panel.name}.</span>{" "}
          <span className="text-fg-muted">{panel.text}</span>
        </li>
      ))}
    </ul>
  );
}
