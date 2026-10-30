import {
  Badge,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  EmptyState,
  Input,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Toggle,
} from "@opendeck/design-system";
import { Pause, Play } from "lucide-react";
import { useEffect, useMemo, useRef } from "react";
import { decodeLabel, entryMatches, formatBytes, formatTime } from "../lib/midi";
import { useAppStore } from "../state/app";
import { setMonitor, useMidiStore } from "../state/midi";

/** Lines drawn at once; the log keeps more in memory than it shows. */
export const MONITOR_VISIBLE = 400;

/** Raw MIDI in and out with the mapped control, in a dialog opened from the top bar. */
export function MidiMonitorDialog() {
  const open = useAppStore((s) => s.midiMonitorOpen);
  const setOpen = useAppStore((s) => s.setMidiMonitorOpen);

  // The backend only streams while the monitor is on screen.
  useEffect(() => {
    if (!open) {
      return;
    }
    void setMonitor(true);
    return () => void setMonitor(false);
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="flex h-[70vh] flex-col sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>MIDI monitor</DialogTitle>
          <DialogDescription>
            Messages from and to the controller, with the control each one is mapped to.
          </DialogDescription>
        </DialogHeader>
        {open ? <MidiMonitor /> : null}
      </DialogContent>
    </Dialog>
  );
}

/** The monitor's toolbar and log. */
export function MidiMonitor() {
  const entries = useMidiStore((s) => s.monitor);
  const origin = useMidiStore((s) => s.monitorOrigin);
  const paused = useMidiStore((s) => s.monitorPaused);
  const filter = useMidiStore((s) => s.monitorFilter);
  const setPaused = useMidiStore((s) => s.setMonitorPaused);
  const setFilter = useMidiStore((s) => s.setMonitorFilter);
  const clear = useMidiStore((s) => s.clearMonitor);

  const matching = useMemo(() => entries.filter((e) => entryMatches(e, filter)), [entries, filter]);
  const shown = matching.slice(-MONITOR_VISIBLE);

  // Follow the newest line unless paused.
  const endRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!paused) {
      endRef.current?.scrollIntoView({ block: "end" });
    }
  }, [shown.length, paused]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={() => setPaused(!paused)} aria-pressed={paused}>
          {paused ? (
            <Play className="size-4" aria-hidden />
          ) : (
            <Pause className="size-4" aria-hidden />
          )}
          {paused ? "Resume" : "Pause"}
        </Button>
        <Button onClick={clear} disabled={entries.length === 0}>
          Clear
        </Button>
        <Toggle
          variant="outline"
          pressed={filter.showIn}
          onPressedChange={(showIn) => setFilter({ showIn })}
          aria-label="Show messages in"
        >
          In
        </Toggle>
        <Toggle
          variant="outline"
          pressed={filter.showOut}
          onPressedChange={(showOut) => setFilter({ showOut })}
          aria-label="Show messages out"
        >
          Out
        </Toggle>
        <Input
          aria-label="Filter messages"
          placeholder="Filter bytes, text or control"
          value={filter.text}
          onChange={(e) => setFilter({ text: e.target.value })}
          className="min-w-48 flex-1"
        />
        <span className="text-xs text-fg-muted" data-testid="monitor-count">
          {matching.length} of {entries.length}
          {paused ? ", paused" : ""}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto rounded-md border border-border bg-bg-subtle">
        {shown.length === 0 ? (
          <EmptyState
            title="No messages"
            description={
              entries.length === 0
                ? "Move something on the controller."
                : "Nothing matches the filter."
            }
          />
        ) : (
          <Table aria-label="MIDI messages" className="font-mono">
            <TableHeader>
              <TableRow>
                <TableHead>Time</TableHead>
                <TableHead>Dir</TableHead>
                <TableHead>Bytes</TableHead>
                <TableHead>Message</TableHead>
                <TableHead>Mapped to</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {shown.map((entry, i) => {
                const decoded = decodeLabel(entry);
                return (
                  <TableRow key={`${entry.ts_us}-${i}`}>
                    <TableCell className="text-fg-muted">
                      {formatTime(entry.ts_us, origin ?? entry.ts_us)}
                    </TableCell>
                    <TableCell>
                      <Badge variant={entry.direction === "in" ? "accent" : "neutral"}>
                        {entry.direction === "in" ? "IN" : "OUT"}
                      </Badge>
                    </TableCell>
                    <TableCell>{formatBytes(entry.bytes)}</TableCell>
                    <TableCell>{entry.text}</TableCell>
                    <TableCell>
                      <span className="flex items-center gap-1">
                        {decoded.text}
                        {decoded.verify ? (
                          <Badge variant="warning" title={decoded.verify}>
                            verify
                          </Badge>
                        ) : null}
                      </span>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}
