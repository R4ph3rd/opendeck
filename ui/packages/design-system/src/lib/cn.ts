import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/** Visible keyboard focus for every interactive component. Uses --focus-ring. */
export const focusRing =
  "focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus-ring";

/** Shared look for floating menus (dropdown, context, select). */
export const menuContent =
  "z-[var(--z-popover)] min-w-40 overflow-hidden rounded-md border border-border bg-surface-raised p-1 text-fg shadow-md animate-[ds-pop-in_var(--duration-fast)_var(--ease-standard)]";

export const menuItem =
  "relative flex h-[var(--control-md)] cursor-default select-none items-center gap-2 rounded-sm px-2 text-sm text-fg outline-none data-[disabled]:pointer-events-none data-[disabled]:text-fg-subtle data-[highlighted]:bg-surface-hover data-[highlighted]:outline-2 data-[highlighted]:-outline-offset-2 data-[highlighted]:outline-focus-ring";
