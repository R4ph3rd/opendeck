import { useEffect, useState } from "react";
import { commands } from "../ipc/bindings";

const urls = new Map<number, string | null>();

/** Object URL of the embedded artwork of a library track, `null` when it has none. */
export function useArtworkUrl(trackId: number | null): string | null {
  const [, refresh] = useState(0);

  useEffect(() => {
    if (trackId === null || urls.has(trackId)) {
      return undefined;
    }
    let cancelled = false;
    void (async () => {
      let next: string | null = null;
      try {
        const result = await commands.getArtwork(trackId);
        if (result.status === "ok" && result.data.length > 0) {
          next = URL.createObjectURL(new Blob([new Uint8Array(result.data)]));
        }
      } catch {
        // No backend or no artwork: show the placeholder.
      }
      urls.set(trackId, next);
      if (!cancelled) {
        refresh((n) => n + 1);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [trackId]);

  return trackId === null ? null : (urls.get(trackId) ?? null);
}
