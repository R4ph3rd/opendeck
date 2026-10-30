import type { ComponentProps, ReactNode } from "react";
import { cn, focusRing } from "../lib/cn";

export interface SettingsLayoutProps extends ComponentProps<"div"> {
  /** Usually a `SettingsNav`. */
  sidebar: ReactNode;
}

/** Sidebar navigation plus a scrolling content pane. Fill the parent's height. */
export function SettingsLayout({ sidebar, children, className, ...props }: SettingsLayoutProps) {
  return (
    <div
      className={cn("flex size-full min-h-0 bg-surface text-fg", className)}
      {...props}
    >
      <aside className="w-48 shrink-0 overflow-auto border-r border-border bg-bg-subtle p-2">
        {sidebar}
      </aside>
      <main className="min-w-0 flex-1 overflow-auto p-6">
        <div className="mx-auto flex max-w-2xl flex-col gap-8">{children}</div>
      </main>
    </div>
  );
}

export interface SettingsNavItem {
  id: string;
  label: string;
  icon?: ReactNode;
}

export interface SettingsNavProps {
  items: SettingsNavItem[];
  value: string;
  onValueChange: (id: string) => void;
  "aria-label"?: string;
}

/** Vertical section list. The active item carries `aria-current="page"`. */
export function SettingsNav({
  items,
  value,
  onValueChange,
  "aria-label": ariaLabel = "Settings sections",
}: SettingsNavProps) {
  return (
    <nav aria-label={ariaLabel}>
      <ul className="flex flex-col gap-px">
        {items.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              aria-current={item.id === value ? "page" : undefined}
              onClick={() => onValueChange(item.id)}
              className={cn(
                "flex h-[var(--control-md)] w-full items-center gap-2 rounded-md px-2 text-left text-sm text-fg-muted [&_svg]:size-4",
                "transition-colors duration-[var(--duration-fast)] ease-standard",
                "hover:bg-surface-hover hover:text-fg aria-[current=page]:bg-accent-muted aria-[current=page]:text-fg",
                focusRing,
              )}
            >
              {item.icon}
              <span className="truncate">{item.label}</span>
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}

export interface SettingsSectionProps extends Omit<ComponentProps<"section">, "title"> {
  title: ReactNode;
  description?: ReactNode;
}

/** A titled group of rows. Labelled by its heading. */
export function SettingsSection({
  title,
  description,
  children,
  className,
  id,
  ...props
}: SettingsSectionProps) {
  const headingId = id ? `${id}-heading` : undefined;
  return (
    <section id={id} aria-labelledby={headingId} className={cn("flex flex-col gap-3", className)} {...props}>
      <header className="flex flex-col gap-1 border-b border-border pb-2">
        <h2 id={headingId} className="text-lg font-semibold text-fg">
          {title}
        </h2>
        {description ? <p className="text-sm text-fg-muted">{description}</p> : null}
      </header>
      <div className="flex flex-col divide-y divide-border">{children}</div>
    </section>
  );
}

export interface SettingsRowProps extends Omit<ComponentProps<"div">, "title"> {
  title: ReactNode;
  description?: ReactNode;
  /** The control, aligned right. */
  control: ReactNode;
}

export function SettingsRow({ title, description, control, className, ...props }: SettingsRowProps) {
  return (
    <div className={cn("flex items-center justify-between gap-6 py-3", className)} {...props}>
      <div className="min-w-0">
        <div className="text-sm font-medium text-fg">{title}</div>
        {description ? <div className="text-xs text-fg-muted">{description}</div> : null}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}
