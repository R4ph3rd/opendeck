import { Toaster } from "@opendeck/design-system";
import { useEffect } from "react";
import { MiddlePanel } from "./components/Placeholders";
import { LibraryPane } from "./components/LibraryPane";
import { DeckPanel } from "./components/DeckPanel";
import { FxPanel } from "./components/fx/FxPanel";
import { MidiMonitorDialog } from "./components/MidiMonitor";
import { MixerPanel } from "./components/MixerPanel";
import { SettingsDialog } from "./components/SettingsDialog";
import { TopBar } from "./components/TopBar";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { DECK_A, DECK_B } from "./state/app";
import { startEventBridge } from "./state/events";
import { WaveformStack } from "./viz/DeckWaveforms";

/**
 * App layout: top bar, stacked waveforms (deck A above deck B), deck A, the
 * mixer and deck B side by side, the middle (pads) panel and the collection.
 */
export function App() {
  useKeyboardShortcuts();

  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void startEventBridge().then((unlisten) => {
      if (cancelled) {
        unlisten();
      } else {
        stop = unlisten;
      }
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  return (
    <Toaster>
      <div className="flex h-full flex-col bg-bg text-fg">
        <TopBar />
        <main className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-3">
          <WaveformStack decks={[DECK_A, DECK_B]} />
          <div className="flex shrink-0 items-start gap-3">
            <DeckPanel deck={DECK_A} />
            <MixerPanel />
            <DeckPanel deck={DECK_B} />
          </div>
          <FxPanel />
          <MiddlePanel />
          <LibraryPane />
        </main>
        <SettingsDialog />
        <MidiMonitorDialog />
      </div>
    </Toaster>
  );
}
