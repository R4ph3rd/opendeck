import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  Toaster,
  type ResizableLayout,
} from "@opendeck/design-system";
import { useEffect } from "react";
import { PadMiddlePanel } from "./components/pads/PadMiddlePanel";
import { LibraryPane } from "./components/LibraryPane";
import { DeckPanel } from "./components/DeckPanel";
import { FxPanel } from "./components/fx/FxPanel";
import { MidiMonitorDialog } from "./components/MidiMonitor";
import { MixerPanel } from "./components/MixerPanel";
import { SettingsDialog } from "./components/SettingsDialog";
import { TopBar } from "./components/TopBar";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { DECK_A, DECK_B } from "./state/app";
import { readStored, writeStored } from "./lib/persist";
import { commands } from "./ipc/bindings";
import { applyTheme, DEFAULT_LAYOUT, LAYOUT_KEY, usePrefsStore } from "./state/prefs";
import { useShortcutStore } from "./state/shortcuts";
import { notifyWarning } from "./state/notify";
import { FirstRunDialog } from "./components/FirstRun";
import { useVizStore } from "./viz/useTrackData";
import { startEventBridge } from "./state/events";
import { WaveformStack } from "./viz/DeckWaveforms";

export { LAYOUT_KEY };

const defaultLayout = (): ResizableLayout => {
  const stored = readStored<ResizableLayout | null>(LAYOUT_KEY, null);
  return stored && typeof stored === "object" ? stored : DEFAULT_LAYOUT;
};

/**
 * App layout: top bar, stacked waveforms (deck A above deck B), deck A, the
 * mixer and deck B side by side, the middle (pads) panel and the collection.
 */
export function App() {
  useKeyboardShortcuts();
  const theme = usePrefsStore((s) => s.theme);
  const layoutRevision = usePrefsStore((s) => s.layoutRevision);

  useEffect(() => {
    void (async () => {
      await Promise.all([usePrefsStore.getState().hydrate(), useShortcutStore.getState().hydrate()]);
      const zoom = usePrefsStore.getState().waveformZoom;
      for (let deck = 0; deck < 4; deck++) {
        useVizStore.getState().setZoom(deck, zoom);
      }
      try {
        const notices = await commands.getStartupNotices();
        notices.forEach(notifyWarning);
      } catch {
        // No backend (tests, previews): nothing to report.
      }
    })();
  }, []);

  useEffect(() => applyTheme(theme), [theme]);

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
        <main className="min-h-0 flex-1 p-3">
          <ResizablePanelGroup
            key={layoutRevision}
            orientation="vertical"
            id="main-layout"
            defaultLayout={defaultLayout()}
            onLayoutChanged={(layout) => writeStored(LAYOUT_KEY, layout)}
          >
            <ResizablePanel id="stage" defaultSize="45%" minSize="15%">
              <div className="flex h-full min-h-0 flex-col gap-3 overflow-auto pb-3">
                <WaveformStack decks={[DECK_A, DECK_B]} />
                <div className="flex shrink-0 items-start gap-3">
                  <DeckPanel deck={DECK_A} />
                  <MixerPanel />
                  <DeckPanel deck={DECK_B} />
                </div>
                <FxPanel />
              </div>
            </ResizablePanel>
            <ResizableHandle withGrip />
            <ResizablePanel id="pads" defaultSize="25%" minSize="10%">
              <div className="h-full py-3">
                <PadMiddlePanel />
              </div>
            </ResizablePanel>
            <ResizableHandle withGrip />
            <ResizablePanel id="library" defaultSize="30%" minSize="10%">
              <div className="flex h-full flex-col pt-3">
                <LibraryPane />
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        </main>
        <SettingsDialog />
        <MidiMonitorDialog />
        <FirstRunDialog />
      </div>
    </Toaster>
  );
}
