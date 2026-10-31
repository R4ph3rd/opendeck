import { Button, KeyValueList } from "@opendeck/design-system";
import { useEffect, useState } from "react";
import { commands, type DiagnosticsDto } from "../../ipc/bindings";
import { copyDiagnostics } from "../../lib/diagnostics";
import { report } from "../../state/actions";
import { notifySuccess } from "../../state/notify";
import { usePrefsStore } from "../../state/prefs";
import { useShortcutStore } from "../../state/shortcuts";
import { Row, Section } from "./parts";

/** Settings, Advanced: logs, diagnostics, database location and reset. */
export function AdvancedSettings() {
  const [info, setInfo] = useState<DiagnosticsDto | null>(null);
  const [confirming, setConfirming] = useState(false);
  const openFirstRun = usePrefsStore((s) => s.openFirstRun);

  useEffect(() => {
    let live = true;
    void report(commands.getDiagnostics()).then((d) => {
      if (live) setInfo(d);
    });
    return () => {
      live = false;
    };
  }, []);

  const reset = async () => {
    const removed = await report(commands.resetUiPrefs());
    setConfirming(false);
    if (removed !== null) {
      usePrefsStore.getState().resetAll();
      useShortcutStore.getState().resetAll();
      notifySuccess("Interface, keyboard and library options are back to defaults.");
    }
  };

  return (
    <>
      <Section title="Logs and diagnostics">
        <Row label="Log files" hint={info?.paths.logs ?? "Not available"}>
          <Button onClick={() => void report(commands.openLogsFolder())}>Open logs folder</Button>
        </Row>
        <Row label="Diagnostics" hint="Version, system, audio and MIDI devices, last errors.">
          <Button onClick={() => void copyDiagnostics()}>Copy diagnostics</Button>
        </Row>
        {info ? (
          <KeyValueList
            items={[
              { label: "Version", value: info.version },
              { label: "System", value: `${info.os} ${info.arch}` },
              { label: "Audio", value: info.audio_device ?? "off" },
              { label: "MIDI", value: info.midi_device ?? "none" },
            ]}
          />
        ) : null}
      </Section>

      <Section title="Database">
        <Row label="Library database" hint="Back up this file to keep your collection.">
          <span
            className="block max-w-80 truncate font-mono text-xs"
            title={info?.paths.database ?? undefined}
            data-testid="database-path"
          >
            {info?.paths.database ?? "In memory"}
          </span>
        </Row>
      </Section>

      <Section title="Reset">
        <Row label="First-run guide" hint="Audio, controller and library setup.">
          <Button onClick={openFirstRun}>Show again</Button>
        </Row>
        <Row
          label="Reset settings"
          hint="Interface, keyboard and library options. Audio, MIDI and your collection stay."
        >
          {confirming ? (
            <div className="flex gap-2">
              <Button variant="danger" onClick={() => void reset()}>
                Confirm reset
              </Button>
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                Cancel
              </Button>
            </div>
          ) : (
            <Button onClick={() => setConfirming(true)}>Reset settings</Button>
          )}
        </Row>
      </Section>
    </>
  );
}
