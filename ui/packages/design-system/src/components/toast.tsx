import { cva } from "class-variance-authority";
import { X } from "lucide-react";
import { Toast as Primitive } from "radix-ui";
import { useSyncExternalStore, type ReactNode } from "react";
import { cn } from "../lib/cn";

export type ToastVariant = "info" | "success" | "warning" | "danger";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface ToastOptions {
  title: string;
  description?: string;
  variant?: ToastVariant;
  action?: ToastAction;
  /** Milliseconds before auto-dismiss. Falls back to the Toaster default. */
  duration?: number;
}

interface ToastItem extends ToastOptions {
  id: string;
  variant: ToastVariant;
}

let items: ToastItem[] = [];
let counter = 0;
const listeners = new Set<() => void>();

function emit(): void {
  for (const l of listeners) l();
}
function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}
const getSnapshot = (): ToastItem[] => items;

/** Show a toast from anywhere. Returns its id. Needs a mounted `Toaster`. */
export function toast(options: ToastOptions): string {
  const id = `toast-${++counter}`;
  items = [...items, { ...options, id, variant: options.variant ?? "info" }];
  emit();
  return id;
}

/** Dismiss one toast by id, or all when omitted. */
toast.dismiss = (id?: string): void => {
  items = id ? items.filter((t) => t.id !== id) : [];
  emit();
};

const rootVariants = cva(
  [
    "pointer-events-auto relative grid grid-cols-[1fr_auto] items-start gap-x-3 gap-y-1 rounded-md border border-l-2 border-border bg-surface-raised p-3 text-sm text-fg shadow-md",
    "data-[state=open]:animate-[ds-pop-in_var(--duration-base)_var(--ease-standard)]",
    "data-[state=closed]:animate-[ds-fade-out_var(--duration-fast)_var(--ease-standard)_forwards]",
  ],
  {
    variants: {
      variant: {
        info: "border-l-accent",
        success: "border-l-success",
        warning: "border-l-warning",
        danger: "border-l-danger",
      },
    },
    defaultVariants: { variant: "info" },
  },
);

export interface ToasterProps {
  /** Default auto-dismiss in ms. */
  duration?: number;
  /** Accessible name of the region. */
  label?: string;
  children?: ReactNode;
}

/** Mount once near the app root. Renders every toast fired through `toast()`. */
export function Toaster({ duration = 5000, label = "Notifications", children }: ToasterProps) {
  const list = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return (
    <Primitive.Provider duration={duration} label={label} swipeDirection="right">
      {children}
      {list.map((t) => (
        <Primitive.Root
          key={t.id}
          duration={t.duration}
          type={t.variant === "danger" || t.variant === "warning" ? "foreground" : "background"}
          className={rootVariants({ variant: t.variant })}
          data-variant={t.variant}
          onOpenChange={(open) => {
            if (!open) toast.dismiss(t.id);
          }}
        >
          <Primitive.Title className="font-medium">{t.title}</Primitive.Title>
          <Primitive.Close
            aria-label="Dismiss"
            className="row-span-2 rounded-sm p-0.5 text-fg-muted hover:bg-surface-hover hover:text-fg focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus-ring"
          >
            <X className="size-3.5" aria-hidden />
          </Primitive.Close>
          {t.description ? (
            <Primitive.Description className="col-start-1 text-fg-muted">
              {t.description}
            </Primitive.Description>
          ) : null}
          {t.action ? (
            <Primitive.Action
              altText={t.action.label}
              onClick={t.action.onClick}
              className={cn(
                "col-start-1 mt-1 justify-self-start rounded-sm border border-border px-2 py-0.5 text-xs font-medium text-fg",
                "hover:bg-surface-hover focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus-ring",
              )}
            >
              {t.action.label}
            </Primitive.Action>
          ) : null}
        </Primitive.Root>
      ))}
      <Primitive.Viewport className="fixed bottom-0 right-0 z-[var(--z-toast)] m-4 flex w-96 max-w-[calc(100vw-2rem)] list-none flex-col gap-2 outline-none" />
    </Primitive.Provider>
  );
}
