import type {
  ControlAction,
  MidiBindingDto,
  MidiMappingRowDto,
  MidiMonitorEntryDto,
} from "../ipc/bindings";

const HEX_WIDTH = 2;
const HEX_RADIX = 16;
const MICROS_PER_MS = 1000;

/** Two digit upper case hex, `0B`. */
export function hex(value: number): string {
  return value.toString(HEX_RADIX).toUpperCase().padStart(HEX_WIDTH, "0");
}

/** Raw bytes as `90 0B 7F`. */
export function formatBytes(bytes: readonly number[]): string {
  return bytes.map(hex).join(" ");
}

/** A binding as it reads in the message list: `Note 0x0B ch1`. */
export function formatBinding(binding: MidiBindingDto): string {
  switch (binding.type) {
    case "note":
      return `Note 0x${hex(binding.number)} ch${binding.channel}`;
    case "cc":
      return `CC 0x${hex(binding.number)} ch${binding.channel}`;
    case "cc14":
      return `CC14 0x${hex(binding.msb)}/0x${hex(binding.lsb)} ch${binding.channel}`;
  }
}

/** Compares two bindings. */
export function sameBinding(a: MidiBindingDto, b: MidiBindingDto): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

const ACTION_NAME: Record<ControlAction["action"], string> = {
  play_pause: "Play/Pause",
  cue: "Cue",
  return_to_start: "Return to start",
  sync: "Sync",
  sync_leader: "Sync leader",
  load: "Load selected track",
  tempo_fader: "Tempo fader",
  tempo_range_next: "Tempo range",
  key_lock: "Key lock",
  pitch_bend: "Pitch bend",
  jog_touch: "Jog touch",
  jog_top: "Jog platter",
  jog_side: "Jog side",
  jog_search: "Jog search",
  vinyl_mode: "Vinyl mode",
  loop_in: "Loop in",
  loop_out: "Loop out",
  loop_toggle: "Loop on/off",
  auto_loop: "Auto loop",
  loop_halve: "Loop halve",
  loop_double: "Loop double",
  beat_jump: "Beat jump",
  cue_loop_call_prev: "Previous memory cue",
  cue_loop_call_next: "Next memory cue",
  memory_cue_add: "Add memory cue",
  memory_cue_delete: "Delete memory cue",
  tap_tempo: "Tap tempo",
  slip: "Slip",
  quantize: "Quantize",
  pad_mode_select: "Pad mode",
  pad: "Pad",
  hot_cue: "Hot cue",
  hot_cue_delete: "Delete hot cue",
  trim: "Trim",
  eq: "EQ",
  filter: "CFX",
  channel_fader: "Channel fader",
  channel_fader_start: "Fader start",
  pfl: "Headphone cue",
  crossfader: "Crossfader",
  master_level: "Master level",
  master_cue: "Master cue",
  headphone_level: "Headphone level",
  headphone_mix: "Headphone mix",
  fx_toggle: "FX on/off",
  fx_level: "FX level/depth",
  fx_select: "FX select",
  fx_beat: "FX beat",
  fx_release: "FX release",
  fx_bpm_auto: "FX BPM auto",
  fx_bpm_tap: "FX BPM tap",
  fx_assign: "FX channel select",
  sampler: "Sampler",
  browse_scroll: "Browse",
  browse_press: "Browse press",
  browse_back: "Browse back",
};

const PAD_MODE_NAME: Record<string, string> = {
  hot_cue: "Hot cue",
  pad_fx1: "Pad FX 1",
  beat_jump: "Beat jump",
  sampler: "Sampler",
  keyboard: "Keyboard",
  pad_fx2: "Pad FX 2",
  beat_loop: "Beat loop",
  key_shift: "Key shift",
};

type Loose = Record<string, unknown>;

function deckPrefix(action: Loose): string {
  const deck = action.deck;
  return typeof deck === "number" ? `Deck ${deck + 1} ` : "";
}

/** Short text for what a control does, e.g. `Deck 1 EQ high`. */
export function describeAction(action: ControlAction): string {
  const loose = action as unknown as Loose;
  let text = `${deckPrefix(loose)}${ACTION_NAME[action.action] ?? action.action}`;
  const detail: string[] = [];
  if (typeof loose.band === "string") {
    detail.push(loose.band);
  }
  if (typeof loose.mode === "string") {
    detail.push(PAD_MODE_NAME[loose.mode] ?? loose.mode);
  }
  if (typeof loose.index === "number" && action.action !== "fx_toggle") {
    detail.push(String(loose.index + 1));
  }
  if (typeof loose.length === "number") {
    detail.push(`${loose.length} beats`);
  }
  if (typeof loose.beats === "number") {
    detail.push(`${loose.beats > 0 ? "+" : ""}${loose.beats} beats`);
  }
  if (typeof loose.direction === "number") {
    detail.push(loose.direction > 0 ? "next" : "previous");
  }
  if (loose.shifted === true) {
    detail.push("shift");
  }
  if (typeof loose.target === "number") {
    detail.push(`ch${loose.target + 1}`);
  }
  if (detail.length > 0) {
    text += ` ${detail.join(", ")}`;
  }
  return text;
}

/** Case-insensitive match of a mapping row against a search text. */
export function rowMatches(row: MidiMappingRowDto, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return true;
  }
  const haystack = [
    row.id,
    row.label ?? "",
    row.source_ref ?? "",
    formatBinding(row.binding),
    describeAction(row.action),
    row.layer,
    row.verify ? "verify" : "",
  ]
    .join("\n")
    .toLowerCase();
  return haystack.includes(needle);
}

/** What the monitor shows in its "mapped to" column. */
export function decodeLabel(entry: MidiMonitorEntryDto): { text: string; verify: string | null } {
  const label = entry.label;
  switch (label.kind) {
    case "control":
      return {
        text: `${label.id}${label.action ? `: ${describeAction(label.action)}` : ""}`,
        verify: label.verify,
      };
    case "modifier":
      return { text: `${label.id} (held)`, verify: label.verify };
    case "pending":
      return { text: `${label.id} (waiting for the other half)`, verify: null };
    case "known_unmapped":
      return { text: `unmapped ${label.source_ref}: ${label.reason}`, verify: null };
    case "unmapped":
      return { text: "not in the mapping", verify: null };
    case "ignored":
      return { text: "ignored", verify: null };
    case "output":
      return { text: "", verify: null };
  }
}

/** Timestamp as `12.345 s` relative to `origin` microseconds. */
export function formatTime(tsUs: number, originUs: number): string {
  const ms = Math.max(0, tsUs - originUs) / MICROS_PER_MS;
  return `${(ms / MICROS_PER_MS).toFixed(3)} s`;
}

export interface MonitorFilter {
  text: string;
  showIn: boolean;
  showOut: boolean;
}

export const NO_MONITOR_FILTER: MonitorFilter = { text: "", showIn: true, showOut: true };

/** Applies direction toggles and the search text to a monitor entry. */
export function entryMatches(entry: MidiMonitorEntryDto, filter: MonitorFilter): boolean {
  if (entry.direction === "in" ? !filter.showIn : !filter.showOut) {
    return false;
  }
  const needle = filter.text.trim().toLowerCase();
  if (needle === "") {
    return true;
  }
  const haystack = `${formatBytes(entry.bytes)}\n${entry.text}\n${decodeLabel(entry).text}`;
  return haystack.toLowerCase().includes(needle);
}
