/** Reads a JSON value from `localStorage`; any failure (blocked, corrupt, missing) gives `fallback`. */
export function readStored<T>(key: string, fallback: T): T {
  try {
    const raw = globalThis.localStorage?.getItem(key);
    return raw === null || raw === undefined ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

/** Writes a JSON value to `localStorage`; failures are ignored. */
export function writeStored(key: string, value: unknown): void {
  try {
    globalThis.localStorage?.setItem(key, JSON.stringify(value));
  } catch {
    // Storage may be full or blocked; the setting then lasts for the session.
  }
}
