import { useEffect } from "react";
import { CollectionPanel, DeckSlot, MiddlePanel } from "./components/Placeholders";
import { DeckPanel } from "./components/DeckPanel";
import { SettingsDialog } from "./components/SettingsDialog";
import { ToastHost } from "./components/ToastHost";
import { TopBar } from "./components/TopBar";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { DECK_A } from "./state/app";
import { startEventBridge } from "./state/events";

/**
 * App layout: top bar, two deck slots, the middle (pads) panel and the
 * collection. Only deck A is live in milestone 1.
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
    <div className="flex h-full flex-col bg-bg text-fg">
      <TopBar />
      <main className="flex min-h-0 flex-1 flex-col gap-3 p-3">
        <div className="grid grid-cols-2 gap-3">
          <DeckPanel deck={DECK_A} />
          <DeckSlot deck={1} />
        </div>
        <MiddlePanel />
        <CollectionPanel />
      </main>
      <SettingsDialog />
      <ToastHost />
    </div>
  );
}
