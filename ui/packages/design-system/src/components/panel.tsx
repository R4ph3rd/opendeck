import type { ComponentProps, ReactNode } from "react";
import { cn } from "../lib/cn";

export interface PanelProps extends ComponentProps<"section"> {
  /** Edge-to-edge band: no border and square corners, separated by a resize handle instead. */
  flush?: boolean;
}

export function Panel({ className, flush = false, ...props }: PanelProps) {
  return (
    <section
      className={cn(
        "flex min-h-0 flex-col overflow-hidden bg-surface text-fg",
        flush ? "rounded-none border-0" : "rounded-lg border border-border",
        className,
      )}
      {...props}
    />
  );
}

export interface PanelHeaderProps extends Omit<ComponentProps<"header">, "title"> {
  title: ReactNode;
  /** Trailing controls, usually IconButtons. */
  actions?: ReactNode;
}

export function PanelHeader({ title, actions, className, ...props }: PanelHeaderProps) {
  return (
    <header
      className={cn(
        "flex h-8 shrink-0 items-center justify-between gap-2 border-b border-border bg-panel-header px-3",
        className,
      )}
      {...props}
    >
      <h2 className="truncate text-xs font-semibold uppercase tracking-wide text-fg-muted">
        {title}
      </h2>
      {actions ? <div className="flex items-center gap-1">{actions}</div> : null}
    </header>
  );
}

export function PanelBody({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("min-h-0 flex-1 overflow-auto p-3", className)} {...props} />;
}
