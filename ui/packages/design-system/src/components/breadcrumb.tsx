import { ChevronRight } from "lucide-react";
import { Slot } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

export function Breadcrumb({ className, ...props }: ComponentProps<"nav">) {
  return <nav aria-label="Breadcrumb" className={className} {...props} />;
}

export function BreadcrumbList({ className, ...props }: ComponentProps<"ol">) {
  return (
    <ol
      className={cn("flex flex-wrap items-center gap-1 text-sm text-fg-muted", className)}
      {...props}
    />
  );
}

export function BreadcrumbItem({ className, ...props }: ComponentProps<"li">) {
  return <li className={cn("inline-flex items-center gap-1", className)} {...props} />;
}

/** A parent folder. Use `asChild` to wrap a router link or a button. */
export function BreadcrumbLink({
  asChild,
  className,
  ...props
}: ComponentProps<"a"> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "a";
  return (
    <Comp
      className={cn(
        "cursor-pointer rounded-sm px-1 transition-colors duration-[var(--duration-fast)] ease-standard hover:text-fg",
        focusRing,
        className,
      )}
      {...props}
    />
  );
}

/** The current location. Not interactive. */
export function BreadcrumbPage({ className, ...props }: ComponentProps<"span">) {
  return <span aria-current="page" className={cn("px-1 font-medium text-fg", className)} {...props} />;
}

export function BreadcrumbSeparator({ children, className, ...props }: ComponentProps<"li">) {
  return (
    <li
      role="presentation"
      aria-hidden
      className={cn("inline-flex text-fg-subtle [&_svg]:size-3.5", className)}
      {...props}
    >
      {children ?? <ChevronRight />}
    </li>
  );
}
