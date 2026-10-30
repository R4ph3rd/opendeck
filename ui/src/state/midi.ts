import { create } from "zustand";
import type {
  MidiBindingDto,
  MidiLearnResultDto,
  MidiMappingRowDto,
  MidiMonitorEntryDto,
  MidiPortDto,
  MidiStatusDto,
} from "../ipc/bindings";
import { commands, type CommandError } from "../ipc/bindings";
import { NO_MONITOR_FILTER, type MonitorFilter } from "../lib/midi";
import { report } from "./actions";
import { notifyError, notifySuccess } from "./notify";

/** Monitor lines kept; the oldest go first. */
export const MONITOR_LIMIT = 5000;

export interface MidiState {
  status: MidiStatusDto | null;
  ports: MidiPortDto[];
  mapping: MidiMappingRowDto[];
  /** Control id waiting for a gesture, `null` when not learning. */
  learning: string | null;
  monitor: MidiMonitorEntryDto[];
  /** Microsecond timestamp of the first monitored message, the time origin. */
  monitorOrigin: number | null;
  monitorPaused: boolean;
  monitorFilter: MonitorFilter;
  setStatus: (status: MidiStatusDto) => void;
  setPorts: (ports: MidiPortDto[]) => void;
  setMapping: (rows: MidiMappingRowDto[]) => void;
  setLearning: (control: string | null) => void;
  /** Adds a batch from `midi-monitor`; dropped while paused. */
  pushMonitor: (batch: MidiMonitorEntryDto[]) => void;
  clearMonitor: () => void;
  setMonitorPaused: (paused: boolean) => void;
  setMonitorFilter: (patch: Partial<MonitorFilter>) => void;
  reset: () => void;
}

const initial = () => ({
  status: null as MidiStatusDto | null,
  ports: [] as MidiPortDto[],
  mapping: [] as MidiMappingRowDto[],
  learning: null as string | null,
  monitor: [] as MidiMonitorEntryDto[],
  monitorOrigin: null as number | null,
  monitorPaused: false,
  monitorFilter: NO_MONITOR_FILTER,
});

export const useMidiStore = create<MidiState>()((set) => ({
  ...initial(),
  setStatus: (status) => set({ status }),
  setPorts: (ports) => set({ ports }),
  setMapping: (mapping) => set({ mapping }),
  setLearning: (learning) => set({ learning }),
  pushMonitor: (batch) =>
    set((state) => {
      if (state.monitorPaused || batch.length === 0) {
        return state;
      }
      const joined = [...state.monitor, ...batch];
      return {
        monitor: joined.length > MONITOR_LIMIT ? joined.slice(joined.length - MONITOR_LIMIT) : joined,
        monitorOrigin: state.monitorOrigin ?? batch[0]?.ts_us ?? null,
      };
    }),
  clearMonitor: () => set({ monitor: [], monitorOrigin: null }),
  setMonitorPaused: (monitorPaused) => set({ monitorPaused }),
  setMonitorFilter: (patch) =>
    set((state) => ({ monitorFilter: { ...state.monitorFilter, ...patch } })),
  reset: () => set(initial()),
}));

const store = () => useMidiStore.getState();

/** True when a command with no result succeeded; an error shows a toast. */
async function succeeded(
  pending: Promise<{ status: "ok"; data: unknown } | { status: "error"; error: CommandError }>,
): Promise<boolean> {
  const outcome = await report(
    pending.then((result) =>
      result.status === "ok" ? { status: "ok" as const, data: true as const } : result,
    ),
  );
  return outcome === true;
}

/** Reads status, ports and the mapping table (settings page open, startup). */
export async function hydrateMidi(): Promise<void> {
  const [status, ports, mapping] = await Promise.all([
    report(commands.midiGetStatus()),
    report(commands.midiListPorts()),
    report(commands.midiGetMapping()),
  ]);
  if (status) {
    store().setStatus(status);
  }
  if (ports) {
    store().setPorts(ports);
  }
  if (mapping) {
    store().setMapping(mapping);
  }
}

/** Re-reads the port list. */
export async function refreshPorts(): Promise<void> {
  const ports = await report(commands.midiListPorts());
  if (ports) {
    store().setPorts(ports);
  }
}

/** Starts the worker; `device` selects a port by name, `null` follows the mapping. */
export async function connectMidi(device: string | null): Promise<void> {
  const status = await report(commands.midiConnect(device));
  if (status) {
    store().setStatus(status);
  }
}

export async function disconnectMidi(): Promise<void> {
  const status = await report(commands.midiDisconnect());
  if (status) {
    store().setStatus(status);
  }
}

export async function setAutoConnect(on: boolean): Promise<void> {
  const status = await report(commands.midiSetAutoConnect(on));
  if (status) {
    store().setStatus(status);
  }
}

export async function setFaderStart(on: boolean): Promise<void> {
  const status = await report(commands.midiSetFaderStart(on));
  if (status) {
    store().setStatus(status);
  }
}

/** Arms MIDI learn for a control. The result arrives as `midi-learned`. */
export async function startLearn(controlId: string): Promise<void> {
  store().setLearning(controlId);
  if (!(await succeeded(commands.midiLearnStart(controlId)))) {
    store().setLearning(null);
  }
}

export async function cancelLearn(): Promise<void> {
  store().setLearning(null);
  await succeeded(commands.midiLearnCancel());
}

/** Applies the outcome of a learn gesture: refresh the table or show why it failed. */
export async function onLearned(result: MidiLearnResultDto): Promise<void> {
  store().setLearning(null);
  if (result.error) {
    notifyError(`${result.control_id}: ${result.error}`);
    return;
  }
  notifySuccess(`${result.control_id} bound to ${result.description}`);
  const rows = await report(commands.midiGetMapping());
  if (rows) {
    store().setMapping(rows);
  }
}

/** Binds a control by hand. */
export async function overrideBinding(controlId: string, binding: MidiBindingDto): Promise<void> {
  const rows = await report(commands.midiSetOverride(controlId, binding));
  if (rows) {
    store().setMapping(rows);
  }
}

export async function resetBinding(controlId: string): Promise<void> {
  const rows = await report(commands.midiClearOverride(controlId));
  if (rows) {
    store().setMapping(rows);
  }
}

export async function resetAllBindings(): Promise<void> {
  const rows = await report(commands.midiResetOverrides());
  if (rows) {
    store().setMapping(rows);
  }
}

/** Starts or stops the backend monitor stream. */
export async function setMonitor(on: boolean): Promise<void> {
  await succeeded(commands.midiSetMonitor(on));
}

export async function exportMapping(path: string): Promise<void> {
  if (await succeeded(commands.midiExportMapping(path))) {
    notifySuccess("Mapping exported.");
  }
}

export async function importMapping(path: string): Promise<void> {
  const rows = await report(commands.midiImportMapping(path));
  if (rows) {
    store().setMapping(rows);
    notifySuccess("Mapping imported.");
  }
}
