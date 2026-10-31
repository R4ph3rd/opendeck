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
import { DECK_A, DECK_B, useAppStore } from "./state/app";
import { BAND_MIN_PX, DECK_COLUMN_MIN_PX, px } from "./lib/layout";
import { readStored, writeStored } from "./lib/persist";
import { commands } from "./ipc/bindings";
import {
  applyTheme,
  DEFAULT_FX_LAYOUT,
  DEFAULT_LAYOUT,
  FX_LAYOUT_KEY,
  LAYOUT_KEY,
  usePrefsStore,
} from "./state/prefs";
import { useShortcutStore } from "./state/shortcuts";
import { notifyWarning } from "./state/notify";
import { FirstRunDialog } from "./components/FirstRun";
import { useVizStore } from "./viz/useTrackData";
import { startEventBridge } from "./state/events";
import { WaveformStack } from "./viz/DeckWaveforms";

export { LAYOUT_KEY };

const BANDS = ["waves", "decks", "pads", "library"] as const;
const FX_BANDS = ["waves", "decks", "fx", "pads", "library"] as const;
const DECK_COLUMNS = ["deck-1", "mixer", "deck-2"] as const;

/** Stored layout when it names exactly the bands in `ids`, else `fallback`. */
const storedLayout = (
  key: string,
  ids: readonly string[],
  fallback: ResizableLayout,
): ResizableLayout => {
  const stored = readStored<ResizableLayout | null>(key, null);
  const valid =
    stored !== null &&
    typeof stored === "object" &&
    ids.every((id) => typeof stored[id] === "number") &&
    Object.keys(stored).length === ids.length;
  return valid ? stored : fallback;
};

/** Storage key of the split between deck 1, the mixer and deck 2. */
export const DECKS_LAYOUT_KEY = "opendeck.layout.decks";

const DEFAULT_DECKS_LAYOUT: ResizableLayout = {
  "deck-1": 37,
  mixer: 26,
  "deck-2": 37,
};

/**
 * App layout: a top bar, then edge-to-edge bands stacked with 1px separators
 * you can drag: the zoomed waveforms (deck 1 over deck 2), the decks with the
 * mixer between them, an optional effects band, the pads (deck 1 | deck 2)
 * and the library.
 */
export function App() {
  useKeyboardShortcuts();
  const theme = usePrefsStore((s) => s.theme);
  const layoutRevision = usePrefsStore((s) => s.layoutRevision);
  const fxOpen = useAppStore((s) => s.fxOpen);

  useEffect(() => {
    void (async () => {
      await Promise.all([
        usePrefsStore.getState().hydrate(),
        useShortcutStore.getState().hydrate(),
      ]);
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
        <main className="min-h-0 flex-1">
          <ResizablePanelGroup
            key={`${layoutRevision}-${fxOpen}`}
            orientation="vertical"
            id="main-layout"
            defaultLayout={
              fxOpen
                ? storedLayout(FX_LAYOUT_KEY, FX_BANDS, DEFAULT_FX_LAYOUT)
                : storedLayout(LAYOUT_KEY, BANDS, DEFAULT_LAYOUT)
            }
            onLayoutChanged={(layout) => writeStored(fxOpen ? FX_LAYOUT_KEY : LAYOUT_KEY, layout)}
          >
            <ResizablePanel id="waves" minSize={px(BAND_MIN_PX.waves)}>
              <WaveformStack decks={[DECK_A, DECK_B]} />
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel id="decks" minSize={px(BAND_MIN_PX.decks)}>
              <ResizablePanelGroup
                orientation="horizontal"
                id="decks-layout"
                defaultLayout={storedLayout(DECKS_LAYOUT_KEY, DECK_COLUMNS, DEFAULT_DECKS_LAYOUT)}
                onLayoutChanged={(layout) => writeStored(DECKS_LAYOUT_KEY, layout)}
              >
                <ResizablePanel id="deck-1" minSize={px(DECK_COLUMN_MIN_PX.deck)}>
                  <DeckPanel deck={DECK_A} />
                </ResizablePanel>
                <ResizableHandle />
                <ResizablePanel id="mixer" minSize={px(DECK_COLUMN_MIN_PX.mixer)}>
                  <MixerPanel />
                </ResizablePanel>
                <ResizableHandle />
                <ResizablePanel id="deck-2" minSize={px(DECK_COLUMN_MIN_PX.deck)}>
                  <DeckPanel deck={DECK_B} />
                </ResizablePanel>
              </ResizablePanelGroup>
            </ResizablePanel>
            <ResizableHandle />
            {fxOpen ? (
              <>
                <ResizablePanel id="fx" minSize={px(BAND_MIN_PX.fx)}>
                  <FxPanel />
                </ResizablePanel>
                <ResizableHandle />
              </>
            ) : null}
            <ResizablePanel id="pads" minSize={px(BAND_MIN_PX.pads)}>
              <PadMiddlePanel />
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel id="library" minSize={px(BAND_MIN_PX.library)}>
              <LibraryPane />
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
