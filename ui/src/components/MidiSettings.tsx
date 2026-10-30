import {
  Badge,
  Button,
  Callout,
  EmptyState,
  IconButton,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SettingsRow,
  SettingsSection,
  StatusDot,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@opendeck/design-system";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Crosshair, RefreshCw, RotateCcw, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { formatBinding, describeAction, rowMatches } from "../lib/midi";
import { errorMessage } from "../state/actions";
import {
  cancelLearn,
  connectMidi,
  disconnectMidi,
  exportMapping,
  hydrateMidi,
  importMapping,
  refreshPorts,
  resetAllBindings,
  resetBinding,
  setAutoConnect,
  setFaderStart,
  startLearn,
  useMidiStore,
} from "../state/midi";
import { notifyError } from "../state/notify";

const AUTO = "auto";
const JSON_FILTER = [{ name: "Mapping", extensions: ["json"] }];

/** Settings page for MIDI controllers: devices, connection and the mapping table. */
export function MidiSettings() {
  useEffect(() => {
    void hydrateMidi();
  }, []);
  return (
    <>
      <ControllerSection />
      <MappingSection />
    </>
  );
}

function ControllerSection() {
  const status = useMidiStore((s) => s.status);
  const ports = useMidiStore((s) => s.ports);
  const [choice, setChoice] = useState<string | null>(null);
  const selected = choice ?? status?.device ?? AUTO;
  const inputs = ports.filter((p) => p.has_input);
  const known = selected === AUTO || inputs.some((p) => p.name === selected);

  let label = "Not running";
  let tone: "success" | "warning" | "danger" = "danger";
  if (status?.connected) {
    label = `Connected to ${status.input ?? status.mapping_name}`;
    tone = "success";
  } else if (status?.enabled) {
    label = `Looking for ${status.device ?? status.mapping_name}`;
    tone = "warning";
  }

  const apply = () => void connectMidi(selected === AUTO ? null : selected);

  return (
    <SettingsSection
      id="midi-controller"
      title="Controller"
      description={`Mapping: ${status?.mapping_name ?? "none"}. The controller connects by itself when it is plugged in.`}
    >
      <SettingsRow
        title="Status"
        description={status?.output ? `LED output: ${status.output}` : "No LED output open"}
        control={
          <StatusDot
            state={status?.connected ? "on" : status?.enabled ? "blink" : "off"}
            tone={tone}
            label={label}
          />
        }
      />
      <SettingsRow
        title="Device"
        description="Automatic follows the mapping's device name."
        control={
          <div className="flex items-center gap-2">
            <Select value={selected} onValueChange={setChoice}>
              <SelectTrigger aria-label="MIDI device" className="w-56">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={AUTO}>Automatic</SelectItem>
                {!known ? <SelectItem value={selected}>{selected} (not found)</SelectItem> : null}
                {inputs.map((p) => (
                  <SelectItem key={p.name} value={p.name}>
                    {p.name}
                    {p.matches_mapping ? " (matches mapping)" : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <IconButton aria-label="Refresh MIDI ports" onClick={() => void refreshPorts()}>
              <RefreshCw className="size-4" aria-hidden />
            </IconButton>
          </div>
        }
      />
      <SettingsRow
        title="Connection"
        control={
          <div className="flex gap-2">
            <Button variant="primary" onClick={apply}>
              {status?.enabled ? "Reconnect" : "Connect"}
            </Button>
            <Button onClick={() => void disconnectMidi()} disabled={!status?.enabled}>
              Disconnect
            </Button>
          </div>
        }
      />
      <SettingsRow
        title="Connect at launch"
        description="Start looking for the controller when OpenDeck starts."
        control={
          <Switch
            aria-label="Connect at launch"
            checked={status?.auto_connect ?? true}
            onCheckedChange={(on) => void setAutoConnect(on)}
          />
        }
      />
      <SettingsRow
        title="Fader start"
        description="Moving a channel fader off the bottom plays the deck; returning it pauses."
        control={
          <Switch
            aria-label="Fader start"
            checked={status?.fader_start ?? false}
            onCheckedChange={(on) => void setFaderStart(on)}
          />
        }
      />
      {status?.last_error ? (
        <Callout variant="danger" title="MIDI error" className="my-3">
          {status.last_error}
        </Callout>
      ) : null}
      {status && status.verify_count > 0 ? (
        <Callout variant="warning" title="Unconfirmed values" className="my-3">
          {status.verify_count} values in this mapping are not confirmed on hardware. They are
          marked in the table below.
        </Callout>
      ) : null}
    </SettingsSection>
  );
}

function MappingSection() {
  const rows = useMidiStore((s) => s.mapping);
  const learning = useMidiStore((s) => s.learning);
  const [query, setQuery] = useState("");
  const shown = useMemo(() => rows.filter((r) => rowMatches(r, query)), [rows, query]);
  const changed = rows.filter((r) => r.overridden).length;

  const doExport = async () => {
    try {
      const path = await save({ filters: JSON_FILTER, defaultPath: "mapping.json" });
      if (path) {
        await exportMapping(path);
      }
    } catch (error) {
      notifyError(errorMessage(error));
    }
  };
  const doImport = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: JSON_FILTER });
      if (typeof path === "string") {
        await importMapping(path);
      }
    } catch (error) {
      notifyError(errorMessage(error));
    }
  };

  return (
    <SettingsSection
      id="midi-mapping"
      title="Mapping"
      description="Learn binds a control to the next thing you move on the controller."
    >
      <div className="flex flex-wrap items-center gap-2 py-3">
        <Input
          aria-label="Search the mapping"
          placeholder="Search control, message or action"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="min-w-48 flex-1"
        />
        <Button onClick={() => void doExport()}>Export</Button>
        <Button onClick={() => void doImport()}>Import</Button>
        <Button onClick={() => void resetAllBindings()} disabled={changed === 0}>
          Reset all
        </Button>
      </div>
      {learning ? (
        <Callout variant="info" title="Learning" className="mb-3">
          <span className="flex items-center justify-between gap-3">
            <span>Move or press the control for {learning} on the controller.</span>
            <Button size="sm" onClick={() => void cancelLearn()}>
              Cancel
            </Button>
          </span>
        </Callout>
      ) : null}
      {rows.length === 0 ? (
        <EmptyState title="No mapping loaded" description="The MIDI service is not running." />
      ) : shown.length === 0 ? (
        <EmptyState title="Nothing matches" description="Change the search text." />
      ) : (
        <Table aria-label="MIDI mapping">
          <TableHeader>
            <TableRow>
              <TableHead>Control</TableHead>
              <TableHead>Message</TableHead>
              <TableHead>Action</TableHead>
              <TableHead>Layer</TableHead>
              <TableHead>
                <span className="sr-only">Actions</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.map((row) => (
              <TableRow key={row.id} data-testid={`mapping-row-${row.id}`}>
                <TableCell className="max-w-40 truncate" title={row.label ?? row.id}>
                  {row.id}
                </TableCell>
                <TableCell>
                  <span className="flex items-center gap-1">
                    {formatBinding(row.binding)}
                    {row.overridden ? <Badge variant="accent">custom</Badge> : null}
                    {row.verify ? (
                      <Badge variant="warning" title={row.verify}>
                        verify
                      </Badge>
                    ) : null}
                  </span>
                </TableCell>
                <TableCell>{describeAction(row.action)}</TableCell>
                <TableCell className="text-fg-muted">{row.layer}</TableCell>
                <TableCell>
                  <span className="flex justify-end gap-1">
                    <IconButton
                      aria-label={`Learn ${row.id}`}
                      aria-pressed={learning === row.id}
                      onClick={() => void (learning === row.id ? cancelLearn() : startLearn(row.id))}
                    >
                      {learning === row.id ? (
                        <X className="size-4" aria-hidden />
                      ) : (
                        <Crosshair className="size-4" aria-hidden />
                      )}
                    </IconButton>
                    <IconButton
                      aria-label={`Reset ${row.id}`}
                      disabled={!row.overridden}
                      onClick={() => void resetBinding(row.id)}
                    >
                      <RotateCcw className="size-4" aria-hidden />
                    </IconButton>
                  </span>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </SettingsSection>
  );
}
