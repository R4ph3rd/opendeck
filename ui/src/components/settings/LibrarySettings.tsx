import { Button, NumberField, SegmentedControl, Switch } from "@opendeck/design-system";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderPlus, RefreshCw, X } from "lucide-react";
import { errorMessage } from "../../lib/errors";
import type { KeyNotation } from "../../lib/library";
import { importFolder, reanalyzeAll } from "../../state/libraryActions";
import { useLibraryStore } from "../../state/library";
import { notifyError } from "../../state/notify";
import { usePrefsStore } from "../../state/prefs";
import { Row, Section } from "./parts";

const MIN_BPM = 30;
const MAX_BPM = 300;

/** Settings, Library: music folders, analysis and key notation. */
export function LibrarySettings() {
  const options = usePrefsStore((s) => s.library);
  const setLibrary = usePrefsStore((s) => s.setLibrary);
  const keyNotation = useLibraryStore((s) => s.keyNotation);
  const setKeyNotation = useLibraryStore((s) => s.setKeyNotation);

  const addFolder = async () => {
    try {
      const picked = await open({ directory: true, multiple: false, title: "Add music folder" });
      if (typeof picked !== "string") {
        return;
      }
      if (!options.folders.includes(picked)) {
        setLibrary({ folders: [...options.folders, picked] });
      }
      await importFolder(picked);
    } catch (error) {
      notifyError(errorMessage(error));
    }
  };

  return (
    <>
      <Section title="Music folders" description="Folders you imported. Rescan after adding files.">
        {options.folders.length === 0 ? (
          <p className="text-sm text-fg-muted">No folders yet.</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {options.folders.map((folder) => (
              <li key={folder} className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate font-mono text-xs" title={folder}>
                  {folder}
                </span>
                <Button size="sm" variant="ghost" onClick={() => void importFolder(folder)}>
                  <RefreshCw className="size-3.5" aria-hidden />
                  Rescan
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`Remove ${folder}`}
                  onClick={() => setLibrary({ folders: options.folders.filter((f) => f !== folder) })}
                >
                  <X className="size-3.5" aria-hidden />
                </Button>
              </li>
            ))}
          </ul>
        )}
        <div>
          <Button onClick={() => void addFolder()}>
            <FolderPlus className="size-3.5" aria-hidden />
            Add folder
          </Button>
        </div>
      </Section>

      <Section title="Analysis">
        <Row label="Analyse on import" hint="BPM, key, loudness and waveform for new tracks.">
          <Switch
            aria-label="Analyse on import"
            checked={options.autoAnalyse}
            onCheckedChange={(autoAnalyse) => setLibrary({ autoAnalyse })}
          />
        </Row>
        <Row
          label="BPM range"
          hint="Tempos outside fold into it. Applies to new imports and re-analysis."
        >
          <div className="flex items-center gap-2">
            <NumberField
              aria-label="Lowest BPM"
              value={options.bpmMin}
              min={MIN_BPM}
              max={options.bpmMax - 1}
              step={1}
              onValueChange={(bpmMin) => setLibrary({ bpmMin })}
            />
            <span className="text-fg-muted" aria-hidden>
              to
            </span>
            <NumberField
              aria-label="Highest BPM"
              value={options.bpmMax}
              min={options.bpmMin + 1}
              max={MAX_BPM}
              step={1}
              onValueChange={(bpmMax) => setLibrary({ bpmMax })}
            />
          </div>
        </Row>
        <Row label="Key notation" hint="How keys appear in the collection and on the decks.">
          <SegmentedControl
            aria-label="Key notation"
            value={keyNotation}
            onValueChange={(v) => setKeyNotation(v as KeyNotation)}
            options={[
              { value: "camelot", label: "Camelot" },
              { value: "musical", label: "Musical" },
            ]}
          />
        </Row>
        <Row label="Re-analyse all" hint="Runs again on every track. A locked beat grid stays.">
          <Button onClick={() => void reanalyzeAll()}>Re-analyse all</Button>
        </Row>
      </Section>
    </>
  );
}
