import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@opendeck/design-system";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  commands,
  type AudioDeviceInfo,
  type AudioHostInfo,
  type AudioSettings,
} from "../ipc/bindings";
import {
  bufferOptions,
  channelPairs,
  pairKey,
  pairLabel,
  parsePairKey,
  sampleRateOptions,
} from "../lib/audioOptions";
import { bufferLatencyMs } from "../lib/format";
import { errorMessage, report } from "../state/actions";
import { useAppStore } from "../state/app";

const DEFAULT = "default";
const OFF = "off";

/** Settings dialog. Audio is the only page in milestone 1. */
export function SettingsDialog() {
  const open = useAppStore((s) => s.settingsOpen);
  const setOpen = useAppStore((s) => s.setSettingsOpen);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Changes to audio restart the output.</DialogDescription>
        </DialogHeader>
        <Tabs defaultValue="audio">
          <TabsList>
            <TabsTrigger value="audio">Audio</TabsTrigger>
          </TabsList>
          <TabsContent value="audio">{open ? <AudioPage onClose={() => setOpen(false)} /> : null}</TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-xs font-medium text-fg-muted">
      {label}
      {children}
    </label>
  );
}

function AudioPage({ onClose }: { onClose: () => void }) {
  const status = useAppStore((s) => s.audio);
  const pushToast = useAppStore((s) => s.pushToast);
  const [hosts, setHosts] = useState<AudioHostInfo[]>([]);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [draft, setDraft] = useState<AudioSettings | null>(null);
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    let live = true;
    void (async () => {
      const [h, s] = await Promise.all([report(commands.listAudioHosts()), report(commands.getAudioSettings())]);
      if (live && h) setHosts(h);
      if (live && s) setDraft(s);
    })();
    return () => {
      live = false;
    };
  }, []);

  const host = draft?.host ?? null;
  useEffect(() => {
    let live = true;
    void (async () => {
      const list = await report(commands.listAudioDevices(host));
      if (live) setDevices(list ?? []);
    })();
    return () => {
      live = false;
    };
  }, [host]);

  const device = useMemo(
    () => devices.find((d) => d.name === draft?.device) ?? devices.find((d) => d.is_default),
    [devices, draft?.device],
  );

  if (!draft) {
    return <p className="text-sm text-fg-muted">Loading audio settings...</p>;
  }

  const patch = (change: Partial<AudioSettings>) => setDraft({ ...draft, ...change });
  const rates = sampleRateOptions(device);
  const buffers = bufferOptions(device);
  const pairs = channelPairs(device);
  const rate = draft.sample_rate ?? device?.default_sample_rate ?? status?.sample_rate ?? null;
  const latency =
    draft.buffer_frames && rate ? bufferLatencyMs(draft.buffer_frames, rate) : null;

  const apply = async () => {
    setApplying(true);
    try {
      const result = await report(commands.setAudioSettings(draft));
      if (result) {
        useAppStore.getState().setAudio(result);
        pushToast("info", "Audio output restarted.");
        onClose();
      }
    } catch (error) {
      pushToast("error", errorMessage(error));
    } finally {
      setApplying(false);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <Field label="Host API">
        <Select
          value={draft.host ?? DEFAULT}
          onValueChange={(v) => patch({ host: v === DEFAULT ? null : (v as AudioSettings["host"]), device: null })}
        >
          <SelectTrigger aria-label="Host API">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={DEFAULT}>System default</SelectItem>
            {hosts.map((h) => (
              <SelectItem key={h.kind} value={h.kind} disabled={!h.available}>
                {h.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <Field label="Output device">
        <Select
          value={draft.device ?? DEFAULT}
          onValueChange={(v) => patch({ device: v === DEFAULT ? null : v })}
        >
          <SelectTrigger aria-label="Output device">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={DEFAULT}>Default device</SelectItem>
            {devices.map((d) => (
              <SelectItem key={d.name} value={d.name}>
                {d.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>
      <div className="flex gap-3">
        <div className="flex-1">
          <Field label="Sample rate">
            <Select
              value={draft.sample_rate ? String(draft.sample_rate) : DEFAULT}
              onValueChange={(v) => patch({ sample_rate: v === DEFAULT ? null : Number(v) })}
            >
              <SelectTrigger aria-label="Sample rate">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={DEFAULT}>Device default</SelectItem>
                {rates.map((r) => (
                  <SelectItem key={r} value={String(r)}>
                    {r} Hz
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
        <div className="flex-1">
          <Field label="Buffer size">
            <Select
              value={draft.buffer_frames ? String(draft.buffer_frames) : DEFAULT}
              onValueChange={(v) => patch({ buffer_frames: v === DEFAULT ? null : Number(v) })}
            >
              <SelectTrigger aria-label="Buffer size">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={DEFAULT}>Device default</SelectItem>
                {buffers.map((b) => (
                  <SelectItem key={b} value={String(b)}>
                    {b} frames
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
      </div>
      <div className="flex gap-3">
        <div className="flex-1">
          <Field label="Master output">
            <Select
              value={pairKey(draft.master)}
              onValueChange={(v) => {
                const pair = parsePairKey(v);
                if (pair) patch({ master: pair });
              }}
            >
              <SelectTrigger aria-label="Master output">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {pairs.map((p) => (
                  <SelectItem key={pairKey(p)} value={pairKey(p)}>
                    {pairLabel(p)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
        <div className="flex-1">
          <Field label="Headphone output">
            <Select
              value={draft.headphones ? pairKey(draft.headphones) : OFF}
              onValueChange={(v) => patch({ headphones: v === OFF ? null : parsePairKey(v) })}
            >
              <SelectTrigger aria-label="Headphone output">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={OFF}>Off</SelectItem>
                {pairs.map((p) => (
                  <SelectItem key={pairKey(p)} value={pairKey(p)}>
                    {pairLabel(p)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
        </div>
      </div>
      <p className="text-sm text-fg-muted" data-testid="latency">
        {latency !== null
          ? `Output latency about ${latency.toFixed(1)} ms (${draft.buffer_frames} frames at ${rate} Hz).`
          : "Output latency depends on the device default buffer."}
      </p>
      <p
        className={status?.running ? "text-sm text-fg-muted" : "text-sm text-danger"}
        data-testid="audio-status"
      >
        {status?.running
          ? `Running on ${status.device ?? "the output"} at ${status.sample_rate ?? "?"} Hz${status.stereo_fallback ? ", stereo fallback (device cannot carry the requested outputs)" : ""}.`
          : `Audio is off${status?.error ? `: ${status.error}` : "."}`}
      </p>
      <DialogFooter>
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" loading={applying} onClick={() => void apply()}>
          Apply
        </Button>
      </DialogFooter>
    </div>
  );
}
