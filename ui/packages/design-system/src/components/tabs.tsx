import { Tabs as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, focusRing } from "../lib/cn";

export const Tabs = Primitive.Root;

export function TabsList({ className, ...props }: ComponentProps<typeof Primitive.List>) {
  return (
    <Primitive.List
      className={cn("inline-flex h-8 items-end gap-1 border-b border-border", className)}
      {...props}
    />
  );
}

export function TabsTrigger({ className, ...props }: ComponentProps<typeof Primitive.Trigger>) {
  return (
    <Primitive.Trigger
      className={cn(
        "-mb-px inline-flex h-8 items-center gap-2 border-b-2 border-transparent px-3 text-sm font-medium text-fg-muted",
        "transition-colors duration-[var(--duration-fast)] ease-standard",
        "hover:text-fg data-[state=active]:border-accent data-[state=active]:text-fg",
        "disabled:pointer-events-none disabled:opacity-50",
        focusRing,
        className,
      )}
      {...props}
    />
  );
}

export function TabsContent({ className, ...props }: ComponentProps<typeof Primitive.Content>) {
  return <Primitive.Content className={cn("mt-3", focusRing, className)} {...props} />;
}
