import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
  Toaster,
  type ResizableLayout,
} from "@opendeck/design-system";
import { Fragment, useEffect } from "react";
import { MiddleRow } from "./components/MiddleRow";
import { DeckRow } from "./components/DeckRow";
import { LibraryPane } from "./components/LibraryPane";
import { MidiMonitorDialog } from "./components/MidiMonitor";
import { SettingsDialog } from "./components/SettingsDialog";
import { TopBar } from "./components/TopBar";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { DECK_A, DECK_B, useAppStore } from "./state/app";
import { BAND_MIN_PX, px } from "./lib/layout";
import { applyUiScale, resolveScale } from "./lib/uiScale";
import { readStored, writeStored } from "./lib/persist";
import { commands } from "./ipc/bindings";
import {
  applyTheme,
  DEFAULT_LAYOUT,
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

type Band = "waves" | "decks" | "middle" | "library";
const BAND_ORDER: readonly Band[] = ["waves", "decks", "middle", "library"];

/** Default split for the visible bands: the default sizes rescaled to sum to 100. */
const defaultsFor = (ids: readonly Band[]): ResizableLayout => {
  const total = ids.reduce((sum, id) => sum + (DEFAULT_LAYOUT[id] ?? 0), 0);
  return Object.fromEntries(ids.map((id) => [id, ((DEFAULT_LAYOUT[id] ?? 0) / total) * 100]));
};

/** Stored layout when it names exactly the bands in `ids`, else the defaults. */
const storedLayout = (key: string, ids: readonly Band[]): ResizableLayout => {
  const stored = readStored<ResizableLayout | null>(key, null);
  const valid =
    stored !== null &&
    typeof stored === "object" &&
    ids.every((id) => typeof stored[id] === "number") &&
    Object.keys(stored).length === ids.length;
  return valid ? stored : defaultsFor(ids);
};

/** The main layout is stored per set of visible bands; all four is the base key. */
const layoutKey = (ids: readonly Band[]): string =>
  ids.length === BAND_ORDER.length ? LAYOUT_KEY : `${LAYOUT_KEY}.${ids.join("+")}`;

/**
 * Performance layout, two decks horizontal: a menu row, then edge-to-edge
 * bands stacked with 1px separators you can drag: the zoomed waveforms (deck 1
 * over deck 2), the decks with the slim mixer between them, the middle row
 * (Beat FX, pad modes or sampler, Beat FX) and the browser.
 */
export function App() {
  useKeyboardShortcuts();
  const theme = usePrefsStore((s) => s.theme);
  const layoutRevision = usePrefsStore((s) => s.layoutRevision);
  const wavesOpen = useAppStore((s) => s.wavesOpen);
  const browserOpen = useAppStore((s) => s.browserOpen);
  const bands = BAND_ORDER.filter(
    (id) => (id !== "waves" || wavesOpen) && (id !== "library" || browserOpen),
  );

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

  const uiScale = usePrefsStore((s) => s.uiScale);
  useEffect(() => {
    const update = () => applyUiScale(resolveScale(uiScale, window.innerHeight));
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [uiScale]);

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
            key={`${layoutRevision}-${bands.join("+")}`}
            orientation="vertical"
            id="main-layout"
            defaultLayout={storedLayout(layoutKey(bands), bands)}
            onLayoutChanged={(layout) => writeStored(layoutKey(bands), layout)}
          >
            {bands.map((id, i) => (
              <Fragment key={id}>
                {i > 0 ? <ResizableHandle /> : null}
                <ResizablePanel id={id} minSize={px(BAND_MIN_PX[id])}>
                  {id === "waves" ? <WaveformStack decks={[DECK_A, DECK_B]} /> : null}
                  {id === "decks" ? <DeckRow /> : null}
                  {id === "middle" ? <MiddleRow /> : null}
                  {id === "library" ? <LibraryPane /> : null}
                </ResizablePanel>
              </Fragment>
            ))}
          </ResizablePanelGroup>
        </main>
        <SettingsDialog />
        <MidiMonitorDialog />
        <FirstRunDialog />
      </div>
    </Toaster>
  );
}
