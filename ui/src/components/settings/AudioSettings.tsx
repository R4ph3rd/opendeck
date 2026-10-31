import {
  Badge,
  Button,
  DialogFooter,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Slider,
  Switch,
} from "@opendeck/design-system";
import { Volume2 } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  commands,
  type AudioDeviceInfo,
  type AudioHostInfo,
  type AudioSettings,
  type AudioStatus,
  type ChannelPair,
  type InputState,
  type TalkOverSettings,
  type TestToneOutput,
} from "../../ipc/bindings";
import {
  bufferOptions,
  channelPairs,
  gainDbLabel,
  hostUnavailableReason,
  inputKey,
  inputLabel,
  micOptions,
  pairKey,
  pairLabel,
  parseInputKey,
  parsePairKey,
  sampleRateOptions,
} from "../../lib/audioOptions";
import { bufferLatencyMs } from "../../lib/format";
import { errorMessage, report } from "../../state/actions";
import { useAppStore } from "../../state/app";
import { notifyError, notifySuccess } from "../../state/notify";
import { Row, Section } from "./parts";

const DEFAULT = "default";
const OFF = "off";
/** How often the page refreshes latency and the xrun count. */
const STATUS_POLL_MS = 2000;
/** Highest level the level sliders offer (linear, about +3.5 dB). */
const MAX_LEVEL = 1.5;

interface TalkOver {
  enabled: boolean;
  depth_db: number;
  threshold_db: number;
}

/** Talk-over with every field present (the bindings mark them optional). */
function talkOver(settings: TalkOverSettings | undefined): TalkOver {
  return {
    enabled: settings?.enabled ?? false,
    depth_db: settings?.depth_db ?? 12,
    threshold_db: settings?.threshold_db ?? -40,
  };
}

/** Whether the output needs a restart to match: booth level and talk-over apply live. */
function needsRestart(draft: AudioSettings, applied: AudioSettings | null): boolean {
  const strip = (s: AudioSettings | null) =>
    s ? JSON.stringify({ ...s, booth_level: null, talkover: null }) : null;
  return strip(draft) !== strip(applied);
}

function PairSelect({
  label,
  value,
  pairs,
  allowOff,
  onChange,
}: {
  label: string;
  value: ChannelPair | null;
  pairs: ChannelPair[];
  allowOff: boolean;
  onChange: (pair: ChannelPair | null) => void;
}) {
  return (
    <Select
      value={value ? pairKey(value) : OFF}
      onValueChange={(v) => {
        if (v === OFF) {
          onChange(null);
          return;
        }
        const pair = parsePairKey(v);
        if (pair) onChange(pair);
      }}
    >
      <SelectTrigger aria-label={label} className="w-28">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {allowOff ? <SelectItem value={OFF}>Off</SelectItem> : null}
        {pairs.map((p) => (
          <SelectItem key={pairKey(p)} value={pairKey(p)}>
            {pairLabel(p)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function LevelSlider({
  label,
  value,
  max = MAX_LEVEL,
  onChange,
}: {
  label: string;
  value: number;
  max?: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex w-56 items-center gap-3">
      <Slider
        aria-label={label}
        aria-valuetext={gainDbLabel(value)}
        min={0}
        max={max}
        step={0.01}
        value={[value]}
        onValueChange={([v]) => v !== undefined && onChange(v)}
      />
      <span className="w-16 shrink-0 text-right font-mono text-xs text-fg-muted">{gainDbLabel(value)}</span>
    </div>
  );
}

function DbSlider({
  label,
  value,
  min,
  max,
  unit,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  unit: string;
  onChange: (value: number) => void;
}) {
  const text = `${Math.round(value)} ${unit}`;
  return (
    <div className="flex w-56 items-center gap-3">
      <Slider
        aria-label={label}
        aria-valuetext={text}
        min={min}
        max={max}
        step={1}
        value={[value]}
        onValueChange={([v]) => v !== undefined && onChange(v)}
      />
      <span className="w-16 shrink-0 text-right font-mono text-xs text-fg-muted">{text}</span>
    </div>
  );
}

function Controls({ children }: { children: ReactNode }) {
  return <div className="flex items-center gap-3">{children}</div>;
}

/** The latency line: measured when the backend has timestamps, else an estimate. */
export function latencyText(
  status: AudioStatus | null,
  draft: AudioSettings,
  rate: number | null,
): string {
  if (status?.running && typeof status.latency_ms === "number") {
    const kind = status.latency_measured ? "measured" : "estimated";
    return `Output latency ${status.latency_ms.toFixed(1)} ms (${kind}).`;
  }
  if (draft.buffer_frames && rate) {
    const ms = bufferLatencyMs(draft.buffer_frames, rate);
    return `Output latency about ${ms.toFixed(1)} ms (${draft.buffer_frames} frames at ${rate} Hz).`;
  }
  return "Output latency depends on the device default buffer.";
}

function statusText(status: AudioStatus | null): string {
  if (!status?.running) {
    if (status?.reconnecting) {
      return `Output lost. Reconnecting (attempt ${status.reconnect_attempts ?? 0}).`;
    }
    return `Audio is off${status?.error ? `: ${status.error}` : "."}`;
  }
  const fallback = status.stereo_fallback
    ? ", stereo fallback (device cannot carry the requested outputs)"
    : "";
  return `Running on ${status.device ?? "the output"} at ${status.sample_rate ?? "?"} Hz${fallback}.`;
}

/** Settings, Audio: device, output routing, inputs and talk-over. */
export function AudioSettingsPage({ onClose }: { onClose: () => void }) {
  const status = useAppStore((s) => s.audio);
  const [hosts, setHosts] = useState<AudioHostInfo[]>([]);
  const [devices, setDevices] = useState<AudioDeviceInfo[]>([]);
  const [inputs, setInputs] = useState<AudioDeviceInfo[]>([]);
  const [draft, setDraft] = useState<AudioSettings | null>(null);
  const [applied, setApplied] = useState<AudioSettings | null>(null);
  const [live, setLive] = useState<InputState | null>(null);
  const [applying, setApplying] = useState(false);
  const [tone, setTone] = useState<TestToneOutput | null>(null);

  useEffect(() => {
    let alive = true;
    void (async () => {
      const [h, s, i] = await Promise.all([
        report(commands.listAudioHosts()),
        report(commands.getAudioSettings()),
        report(commands.getInputState()),
      ]);
      if (!alive) return;
      if (h) setHosts(h);
      if (s) {
        setDraft(s);
        setApplied(s);
      }
      if (i) setLive(i);
    })();
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    const timer = setInterval(() => {
      void commands
        .getAudioStatus()
        .then((r) => r.status === "ok" && useAppStore.getState().setAudio(r.data))
        .catch(() => undefined);
    }, STATUS_POLL_MS);
    return () => clearInterval(timer);
  }, []);

  const host = draft?.host ?? null;
  useEffect(() => {
    let alive = true;
    void (async () => {
      const [outputs, ins] = await Promise.all([
        report(commands.listAudioDevices(host)),
        report(commands.listAudioInputDevices(host)),
      ]);
      if (!alive) return;
      setDevices(outputs ?? []);
      setInputs(ins ?? []);
    })();
    return () => {
      alive = false;
    };
  }, [host]);

  const device = useMemo(
    () => devices.find((d) => d.name === draft?.device) ?? devices.find((d) => d.is_default),
    [devices, draft?.device],
  );
  const inputDevice = useMemo(
    () =>
      inputs.find((d) => d.name === (draft?.input_device ?? draft?.device)) ??
      inputs.find((d) => d.is_default),
    [inputs, draft?.input_device, draft?.device],
  );

  if (!draft) {
    return <p className="text-sm text-fg-muted">Loading audio settings...</p>;
  }

  const patch = (change: Partial<AudioSettings>) => setDraft({ ...draft, ...change });
  const rates = sampleRateOptions(device);
  const buffers = bufferOptions(device);
  const pairs = channelPairs(device);
  const micChoices = micOptions(inputDevice);
  const auxPairs = channelPairs(inputDevice);
  const rate = draft.sample_rate ?? device?.default_sample_rate ?? status?.sample_rate ?? null;
  const talk = talkOver(draft.talkover);
  const booth = draft.booth ?? null;
  const mic = draft.mic ?? null;
  const aux = draft.aux ?? null;
  const dirty = needsRestart(draft, applied);

  const applySettings = async (): Promise<boolean> => {
    setApplying(true);
    try {
      const result = await report(commands.setAudioSettings(draft));
      if (!result) return false;
      useAppStore.getState().setAudio(result);
      setApplied(draft);
      return true;
    } catch (error) {
      notifyError(errorMessage(error));
      return false;
    } finally {
      setApplying(false);
    }
  };

  const apply = async () => {
    if (await applySettings()) {
      notifySuccess("Audio output restarted.");
      onClose();
    }
  };

  /** Plays the engine's test tone. Unsaved changes are applied first, since the tone uses them. */
  const test = async (output: TestToneOutput) => {
    setTone(output);
    try {
      if (dirty && !(await applySettings())) return;
      await report(commands.playTestTone(output));
    } finally {
      setTone(null);
    }
  };

  const testButton = (output: TestToneOutput, label: string, routed: boolean) => (
    <Button
      size="sm"
      variant="ghost"
      disabled={!routed || (tone !== null && tone !== output)}
      loading={tone === output}
      aria-label={`Test tone on ${label}`}
      onClick={() => void test(output)}
    >
      <Volume2 className="size-3.5" aria-hidden />
      Test
    </Button>
  );

  const setBooth = (gain: number) => {
    patch({ booth_level: gain });
    void report(commands.setBoothLevel(gain));
  };
  const setTalk = (change: Partial<TalkOverSettings>) => {
    const next = { ...talk, ...change };
    patch({ talkover: next });
    void report(commands.setTalkOver(next));
  };
  const setLiveField = (change: Partial<InputState>, send: () => Promise<unknown>) => {
    if (live) setLive({ ...live, ...change });
    void send();
  };

  return (
    <>
      <Section title="Device" description="Changes here restart the output when you apply them.">
        <Row label="Host API" hint="Unavailable hosts show what they need.">
          <Select
            value={draft.host ?? DEFAULT}
            onValueChange={(v) =>
              patch({ host: v === DEFAULT ? null : (v as AudioSettings["host"]), device: null, input_device: null })
            }
          >
            <SelectTrigger aria-label="Host API" className="w-64">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={DEFAULT}>System default</SelectItem>
              {hosts.map((h) => {
                const reason = hostUnavailableReason(h);
                return (
                  <SelectItem key={h.kind} value={h.kind} disabled={reason !== null}>
                    {reason ? `${h.name}: ${reason}` : h.name}
                  </SelectItem>
                );
              })}
            </SelectContent>
          </Select>
        </Row>
        <Row label="Output device">
          <Select value={draft.device ?? DEFAULT} onValueChange={(v) => patch({ device: v === DEFAULT ? null : v })}>
            <SelectTrigger aria-label="Output device" className="w-64">
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
        </Row>
        <Row label="Sample rate">
          <Select
            value={draft.sample_rate ? String(draft.sample_rate) : DEFAULT}
            onValueChange={(v) => patch({ sample_rate: v === DEFAULT ? null : Number(v) })}
          >
            <SelectTrigger aria-label="Sample rate" className="w-40">
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
        </Row>
        <Row label="Buffer size" hint="Smaller is faster to respond, larger is safer from dropouts.">
          <Select
            value={draft.buffer_frames ? String(draft.buffer_frames) : DEFAULT}
            onValueChange={(v) => patch({ buffer_frames: v === DEFAULT ? null : Number(v) })}
          >
            <SelectTrigger aria-label="Buffer size" className="w-40">
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
        </Row>
        <div className="flex flex-col gap-1 text-sm">
          <p className="text-fg-muted" data-testid="latency">
            {latencyText(dirty ? null : status, draft, rate)}
          </p>
          <p className={status?.running ? "text-fg-muted" : "text-danger"} data-testid="audio-status">
            {statusText(status)}
          </p>
          <div className="flex items-center gap-2 text-fg-muted">
            <span>Dropouts since start</span>
            <Badge variant={(status?.xruns ?? 0) > 0 ? "warning" : "neutral"} data-testid="xruns">
              {status?.xruns ?? 0}
            </Badge>
          </div>
        </div>
      </Section>

      <Section title="Outputs" description="Channel pairs on the output device. Mono plays left plus right on both.">
        <Row label="Master">
          <Controls>
            <PairSelect
              label="Master output"
              value={draft.master}
              pairs={pairs}
              allowOff={false}
              onChange={(p) => p && patch({ master: p })}
            />
            <Switch aria-label="Master mono" checked={draft.master_mono ?? false} onCheckedChange={(v) => patch({ master_mono: v })} />
            <span className="text-xs text-fg-muted">Mono</span>
            {testButton("master", "master", true)}
          </Controls>
        </Row>
        <Row label="Headphones">
          <Controls>
            <PairSelect
              label="Headphone output"
              value={draft.headphones}
              pairs={pairs}
              allowOff
              onChange={(p) => patch({ headphones: p })}
            />
            <Switch
              aria-label="Headphones mono"
              checked={draft.headphones_mono ?? false}
              onCheckedChange={(v) => patch({ headphones_mono: v })}
            />
            <span className="text-xs text-fg-muted">Mono</span>
            {testButton("headphones", "headphones", draft.headphones !== null)}
          </Controls>
        </Row>
        <Row label="Booth" hint="A copy of the master with its own level.">
          <Controls>
            <PairSelect label="Booth output" value={booth} pairs={pairs} allowOff onChange={(p) => patch({ booth: p })} />
            <Switch aria-label="Booth mono" checked={draft.booth_mono ?? false} onCheckedChange={(v) => patch({ booth_mono: v })} />
            <span className="text-xs text-fg-muted">Mono</span>
            {testButton("booth", "booth", booth !== null)}
          </Controls>
        </Row>
        <Row label="Booth level">
          <LevelSlider label="Booth level" value={draft.booth_level ?? 1} max={1} onChange={setBooth} />
        </Row>
      </Section>

      <Section title="Inputs" description="Mic and aux join the master. They must run at the output rate.">
        <Row label="Input device">
          <Select
            value={draft.input_device ?? DEFAULT}
            onValueChange={(v) => patch({ input_device: v === DEFAULT ? null : v })}
          >
            <SelectTrigger aria-label="Input device" className="w-64">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={DEFAULT}>Same as output or default</SelectItem>
              {inputs.map((d) => (
                <SelectItem key={d.name} value={d.name}>
                  {d.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
        <Row label="Mic channels" hint="One channel is played on both sides.">
          <Select
            value={mic ? inputKey(mic) : OFF}
            onValueChange={(v) => patch({ mic: v === OFF ? null : parseInputKey(v) })}
          >
            <SelectTrigger aria-label="Mic channels" className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={OFF}>Off</SelectItem>
              {micChoices.map((c) => (
                <SelectItem key={inputKey(c)} value={inputKey(c)}>
                  {inputLabel(c)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
        <Row label="Aux input">
          <Select
            value={aux ? pairKey(aux) : OFF}
            onValueChange={(v) => patch({ aux: v === OFF ? null : parsePairKey(v) })}
          >
            <SelectTrigger aria-label="Aux input" className="w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={OFF}>Off</SelectItem>
              {auxPairs.map((p) => (
                <SelectItem key={pairKey(p)} value={pairKey(p)}>
                  In {pairLabel(p)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </Row>
        {status?.input_error ? (
          <p className="text-sm text-danger" data-testid="input-error">
            Inputs unavailable: {status.input_error}
          </p>
        ) : null}
        {live ? (
          <>
            <Row label="Mic on">
              <Switch
                aria-label="Mic on"
                checked={live.mic_on}
                onCheckedChange={(on) => setLiveField({ mic_on: on }, () => report(commands.setMicOn(on)))}
              />
            </Row>
            <Row label="Mic level">
              <LevelSlider
                label="Mic level"
                value={live.mic_level ?? 1}
                onChange={(g) => setLiveField({ mic_level: g }, () => report(commands.setMicLevel(g)))}
              />
            </Row>
            <Row label="Mic low" hint="Shelf at 120 Hz.">
              <LevelSlider
                label="Mic low"
                value={live.mic_eq_low ?? 1}
                max={2}
                onChange={(g) => setLiveField({ mic_eq_low: g }, () => report(commands.setMicEq("low", g)))}
              />
            </Row>
            <Row label="Mic high" hint="Shelf at 5 kHz.">
              <LevelSlider
                label="Mic high"
                value={live.mic_eq_high ?? 1}
                max={2}
                onChange={(g) => setLiveField({ mic_eq_high: g }, () => report(commands.setMicEq("high", g)))}
              />
            </Row>
            <Row label="Aux level">
              <LevelSlider
                label="Aux level"
                value={live.aux_level ?? 1}
                onChange={(g) => setLiveField({ aux_level: g }, () => report(commands.setAuxLevel(g)))}
              />
            </Row>
          </>
        ) : null}
      </Section>

      <Section title="Talk-over" description="Lowers decks, sampler and aux while the mic is above the threshold.">
        <Row label="Talk-over">
          <Switch aria-label="Talk-over" checked={talk.enabled} onCheckedChange={(enabled) => setTalk({ enabled })} />
        </Row>
        <Row label="Depth" hint="How far the music drops.">
          <DbSlider
            label="Talk-over depth"
            value={talk.depth_db}
            min={0}
            max={40}
            unit="dB"
            onChange={(depth_db) => setTalk({ depth_db })}
          />
        </Row>
        <Row label="Threshold" hint="Mic level that starts the ducking.">
          <DbSlider
            label="Talk-over threshold"
            value={talk.threshold_db}
            min={-80}
            max={0}
            unit="dBFS"
            onChange={(threshold_db) => setTalk({ threshold_db })}
          />
        </Row>
      </Section>

      <DialogFooter>
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" loading={applying} onClick={() => void apply()}>
          Apply
        </Button>
      </DialogFooter>
    </>
  );
}
