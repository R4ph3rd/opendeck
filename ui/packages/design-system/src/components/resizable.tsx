import type { ComponentProps } from "react";
import { Group, Panel, Separator, type GroupProps, type Layout } from "react-resizable-panels";
import { cn } from "../lib/cn";

export type ResizableLayout = Layout;

export interface ResizablePanelGroupProps
  extends Omit<GroupProps, "onLayoutChange" | "onLayoutChanged"> {
  /**
   * Called once a drag or keyboard resize settles. Persist the layout here and
   * feed it back through `defaultLayout` on the next mount. Keys are panel ids.
   */
  onLayoutChanged?: (layout: ResizableLayout) => void;
}

/** Split container. Give every `ResizablePanel` an `id` so layouts can be persisted. */
export function ResizablePanelGroup({
  className,
  onLayoutChanged,
  ...props
}: ResizablePanelGroupProps) {
  return (
    <Group
      className={cn("size-full", className)}
      {...(onLayoutChanged ? { onLayoutChanged: (layout: ResizableLayout) => onLayoutChanged(layout) } : {})}
      {...props}
    />
  );
}

export function ResizablePanel({ className, ...props }: ComponentProps<typeof Panel>) {
  return <Panel className={cn("min-h-0 min-w-0", className)} {...props} />;
}

/**
 * Drag handle between panels. A 1px line with a wider hit area, accent on
 * hover, drag and keyboard focus. Arrow keys resize, Home/End jump to limits.
 */
export function ResizableHandle({
  className,
  withGrip,
  ...props
}: ComponentProps<typeof Separator> & { withGrip?: boolean }) {
  return (
    <Separator
      className={cn(
        "relative flex shrink-0 items-center justify-center bg-border outline-none",
        "transition-colors duration-[var(--duration-fast)] ease-standard",
        "data-[separator=hover]:bg-accent data-[separator=active]:bg-accent data-[separator=focus]:bg-accent",
        "focus-visible:outline-2 focus-visible:outline-focus-ring",
        "aria-[orientation=vertical]:w-px aria-[orientation=vertical]:after:absolute aria-[orientation=vertical]:after:inset-y-0 aria-[orientation=vertical]:after:-inset-x-1",
        "aria-[orientation=horizontal]:h-px aria-[orientation=horizontal]:after:absolute aria-[orientation=horizontal]:after:inset-x-0 aria-[orientation=horizontal]:after:-inset-y-1",
        className,
      )}
      {...props}
    >
      {withGrip ? (
        <span aria-hidden className="z-10 h-6 w-1 rounded-full bg-border-strong" />
      ) : null}
    </Separator>
  );
}
