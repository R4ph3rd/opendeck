import { useEffect, useState } from "react";
import { commands, type AppInfo } from "./ipc/bindings";

/** Placeholder shell: proves the typed IPC path end to end. */
export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    commands
      .appInfo()
      .then(setInfo)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  return (
    <main>
      <h1>OpenDeck</h1>
      {info && (
        <p>
          v{info.version} on {info.os}/{info.arch}, {info.visible_decks} of {info.max_decks} decks
        </p>
      )}
      {error && <p role="alert">Backend unavailable: {error}</p>}
    </main>
  );
}
