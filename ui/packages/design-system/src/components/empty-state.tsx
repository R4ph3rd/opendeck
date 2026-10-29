import type { ComponentProps, ReactNode } from "react";
import { cn } from "../lib/cn";

export interface EmptyStateProps extends Omit<ComponentProps<"div">, "title"> {
  icon?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  /** Call to action, usually a Button. */
  action?: ReactNode;
}

/** Empty or first-run state with the soft mesh backdrop (an allowed gradient use). */
export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
  ...props
}: EmptyStateProps) {
  return (
    <div
      className={cn(
        "relative isolate flex flex-col items-center justify-center gap-3 overflow-hidden p-8 text-center",
        className,
      )}
      {...props}
    >
      <div
        aria-hidden
        className="bg-mesh absolute inset-0 -z-10 blur-[var(--backdrop-blur-subtle)]"
      />
      {icon ? <div className="text-fg-muted [&_svg]:size-6">{icon}</div> : null}
      <h3 className="text-lg font-semibold text-fg">{title}</h3>
      {description ? <p className="max-w-sm text-sm text-fg-muted">{description}</p> : null}
      {action}
    </div>
  );
}
