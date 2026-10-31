import type {
  AudioDeviceInfo,
  AudioSettings,
  ChannelPair,
  MidiMonitorEntryDto,
  MidiPortDto,
} from "../ipc/bindings";

/** The setup steps in order. */
export const FIRST_RUN_STEPS = ["welcome", "audio", "controller", "library", "tour"] as const;
export type FirstRunStep = (typeof FIRST_RUN_STEPS)[number];

/** DDJ-FLX4 master output (channels 1/2). */
export const FLX4_MASTER: ChannelPair = { left: 0, right: 1 };
/** DDJ-FLX4 headphone output (channels 3/4). */
export const FLX4_HEADPHONES: ChannelPair = { left: 2, right: 3 };

const FLX4_NAME = /flx\s?-?4/i;

/** Whether a device or port name belongs to a DDJ-FLX4. */
export function isFlx4Name(name: string): boolean {
  return FLX4_NAME.test(name);
}

/** The FLX4 among the output devices, if one is listed. */
export function findFlx4Device(devices: readonly AudioDeviceInfo[]): AudioDeviceInfo | undefined {
  return devices.find((d) => isFlx4Name(d.name));
}

/** The FLX4 among the MIDI ports, if one is listed. */
export function findFlx4Port(ports: readonly MidiPortDto[]): MidiPortDto | undefined {
  return ports.find((p) => p.has_input && isFlx4Name(p.name));
}

/** Settings that select the FLX4 with master on 1/2 and headphones on 3/4. */
export function flx4Settings(base: AudioSettings, device: AudioDeviceInfo): AudioSettings {
  return { ...base, device: device.name, master: FLX4_MASTER, headphones: FLX4_HEADPHONES };
}

const NOTE_ON = 0x90;
const STATUS_MASK = 0xf0;

/** A PLAY/PAUSE press on `deck` (zero-based) in the monitor stream. */
export function isPlayPress(entry: MidiMonitorEntryDto, deck: number): boolean {
  const { label, bytes, direction } = entry;
  if (direction !== "in" || label.kind !== "control") {
    return false;
  }
  const action = label.action;
  if (action?.action !== "play_pause" || action.deck !== deck) {
    return false;
  }
  const [status = 0, , velocity = 0] = bytes;
  return (status & STATUS_MASK) === NOTE_ON && velocity > 0;
}

/** Whether any entry is a PLAY press on `deck`. */
export function sawPlayPress(entries: readonly MidiMonitorEntryDto[], deck: number): boolean {
  return entries.some((e) => isPlayPress(e, deck));
}
