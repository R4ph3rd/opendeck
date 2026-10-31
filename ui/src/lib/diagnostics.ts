import { commands } from "../ipc/bindings";
import { notifyError, notifySuccess } from "../state/notify";
import { errorMessage } from "./errors";

async function backendReport(): Promise<string> {
  try {
    const result = await commands.getDiagnostics();
    return result.status === "ok"
      ? result.data.text.trimEnd()
      : `Backend report unavailable: ${errorMessage(result.error)}`;
  } catch (error) {
    return `Backend report unavailable: ${errorMessage(error)}`;
  }
}

/** Plain text of the backend report plus what only the UI knows. */
export async function buildDiagnosticsText(uiError?: { message: string; stack?: string | undefined }): Promise<string> {
  const backend = await backendReport();
  const lines = [backend, `User agent: ${globalThis.navigator?.userAgent ?? "unknown"}`];
  if (uiError) {
    lines.push(`UI error: ${uiError.message}`);
    if (uiError.stack) {
      lines.push(uiError.stack);
    }
  }
  return lines.join("\n");
}

/** Copies the diagnostics report to the clipboard and says so. */
export async function copyDiagnostics(uiError?: { message: string; stack?: string | undefined }): Promise<boolean> {
  const text = await buildDiagnosticsText(uiError);
  try {
    await globalThis.navigator.clipboard.writeText(text);
    notifySuccess("Diagnostics copied.");
    return true;
  } catch (error) {
    notifyError(`Could not copy: ${errorMessage(error)}`);
    return false;
  }
}

/** Shows toasts for errors nothing else caught. Returns a function that removes the handlers. */
export function installGlobalErrorHandlers(): () => void {
  const onError = (event: ErrorEvent) => {
    console.error(event.error ?? event.message);
    notifyError(errorMessage(event.error ?? event.message));
  };
  const onRejection = (event: PromiseRejectionEvent) => {
    console.error(event.reason);
    notifyError(errorMessage(event.reason));
  };
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}
