import { IconButton } from "@opendeck/design-system";
import { CircleAlert, Info, X } from "lucide-react";
import { useAppStore } from "../state/app";

/** Transient messages, bottom right. Errors are announced assertively. */
export function ToastHost() {
  const toasts = useAppStore((s) => s.toasts);
  const dismiss = useAppStore((s) => s.dismissToast);

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[var(--z-toast)] flex w-80 flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role={toast.kind === "error" ? "alert" : "status"}
          className="pointer-events-auto flex items-start gap-2 rounded-md border border-border-strong bg-surface-raised p-3 text-sm text-fg shadow-md"
        >
          {toast.kind === "error" ? (
            <CircleAlert className="mt-px size-4 shrink-0 text-danger" aria-hidden />
          ) : (
            <Info className="mt-px size-4 shrink-0 text-fg-muted" aria-hidden />
          )}
          <p className="min-w-0 flex-1 break-words">{toast.message}</p>
          <IconButton size="sm" aria-label="Dismiss" onClick={() => dismiss(toast.id)}>
            <X className="size-3.5" aria-hidden />
          </IconButton>
        </div>
      ))}
    </div>
  );
}
