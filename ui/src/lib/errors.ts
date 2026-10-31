import type { CommandError } from "../ipc/bindings";

const KINDS: readonly CommandError["kind"][] = ["internal", "audio", "media", "invalid"];

/** Whether `value` is a `CommandError` as the backend serialises it. */
export function isCommandError(value: unknown): value is CommandError {
  if (!value || typeof value !== "object") {
    return false;
  }
  const { kind, message } = value as { kind?: unknown; message?: unknown };
  return typeof message === "string" && KINDS.includes(kind as CommandError["kind"]);
}

/**
 * User-facing text for a typed `CommandError`. Media and argument errors
 * already read as sentences. Audio and internal ones get a short lead so the
 * user knows where to look.
 */
export function describeCommandError(error: CommandError): string {
  switch (error.kind) {
    case "audio":
      return `Audio problem: ${error.message}`;
    case "internal":
      return `OpenDeck hit an internal error: ${error.message} Details are in the log (Settings, Advanced).`;
    default:
      return error.message;
  }
}

/** Human-readable text for whatever a rejected command or a thrown value carries. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (isCommandError(error)) {
    return describeCommandError(error);
  }
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return "Something went wrong.";
}
