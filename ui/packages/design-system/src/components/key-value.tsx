import type { ComponentProps, ReactNode } from "react";
import { cn } from "../lib/cn";

export interface KeyValueItem {
  label: ReactNode;
  value: ReactNode;
}

export interface KeyValueListProps extends Omit<ComponentProps<"dl">, "children"> {
  items: KeyValueItem[];
  /** Render values in the mono face (paths, hashes, sample rates). */
  mono?: boolean;
}

/** Definition list for info and settings summaries: muted label left, value right. */
export function KeyValueList({ items, mono, className, ...props }: KeyValueListProps) {
  return (
    <dl className={cn("divide-y divide-border text-sm", className)} {...props}>
      {items.map((item, i) => (
        <div key={i} className="grid min-h-[var(--row-h)] grid-cols-[minmax(0,1fr)_minmax(0,2fr)] items-center gap-3 py-1">
          <dt className="truncate text-fg-muted">{item.label}</dt>
          <dd className={cn("m-0 min-w-0 break-words text-fg", mono && "font-mono text-xs")}>
            {item.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}
