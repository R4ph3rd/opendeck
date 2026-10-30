import { Command as Primitive } from "cmdk";
import { Check, ChevronsUpDown, Search } from "lucide-react";
import { useState, type ComponentProps, type ReactNode } from "react";
import { cn, focusRing } from "../lib/cn";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "./dialog";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

export function Command({ className, ...props }: ComponentProps<typeof Primitive>) {
  return (
    <Primitive
      className={cn(
        "flex w-full flex-col overflow-hidden rounded-md bg-surface-raised text-fg",
        className,
      )}
      {...props}
    />
  );
}

export function CommandInput({ className, ...props }: ComponentProps<typeof Primitive.Input>) {
  return (
    <div className="flex h-8 items-center gap-2 border-b border-border px-3">
      <Search className="size-4 shrink-0 text-fg-muted" aria-hidden />
      <Primitive.Input
        className={cn(
          "h-full w-full bg-transparent text-sm text-fg outline-none placeholder:text-fg-subtle disabled:opacity-50",
          className,
        )}
        {...props}
      />
    </div>
  );
}

export function CommandList({ className, ...props }: ComponentProps<typeof Primitive.List>) {
  return <Primitive.List className={cn("max-h-72 overflow-auto p-1", className)} {...props} />;
}

export function CommandEmpty({ className, ...props }: ComponentProps<typeof Primitive.Empty>) {
  return (
    <Primitive.Empty
      className={cn("px-2 py-4 text-center text-sm text-fg-muted", className)}
      {...props}
    />
  );
}

export function CommandGroup({ className, ...props }: ComponentProps<typeof Primitive.Group>) {
  return (
    <Primitive.Group
      className={cn(
        "[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-wide [&_[cmdk-group-heading]]:text-fg-muted",
        className,
      )}
      {...props}
    />
  );
}

export function CommandSeparator({
  className,
  ...props
}: ComponentProps<typeof Primitive.Separator>) {
  return <Primitive.Separator className={cn("-mx-1 my-1 h-px bg-border", className)} {...props} />;
}

export function CommandItem({ className, ...props }: ComponentProps<typeof Primitive.Item>) {
  return (
    <Primitive.Item
      className={cn(
        "relative flex h-[var(--control-md)] cursor-default select-none items-center gap-2 rounded-sm px-2 text-sm text-fg outline-none",
        "data-[disabled=true]:pointer-events-none data-[disabled=true]:text-fg-subtle",
        "data-[selected=true]:bg-surface-hover data-[selected=true]:outline-2 data-[selected=true]:-outline-offset-2 data-[selected=true]:outline-focus-ring",
        className,
      )}
      {...props}
    />
  );
}

/** Right-aligned hint, usually one or more Kbd. */
export function CommandShortcut({ className, ...props }: ComponentProps<"span">) {
  return (
    <span
      className={cn("ml-auto flex items-center gap-1 text-xs text-fg-muted", className)}
      {...props}
    />
  );
}

export interface CommandPaletteProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Accessible dialog name. Visually hidden. */
  title?: string;
  children: ReactNode;
}

/** Modal command palette. Bind Mod+K in the app and render Command parts as children. */
export function CommandPalette({
  open,
  onOpenChange,
  title = "Command palette",
  children,
}: CommandPaletteProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="top-[20%] translate-y-0 overflow-hidden p-0 sm:max-w-lg">
        <DialogTitle className="sr-only">{title}</DialogTitle>
        <DialogDescription className="sr-only">
          Type to filter, arrows to move, Enter to run.
        </DialogDescription>
        <Command loop>{children}</Command>
      </DialogContent>
    </Dialog>
  );
}

export interface ComboboxOption {
  value: string;
  label: string;
  /** Extra search terms. */
  keywords?: string[];
  disabled?: boolean;
}

export interface ComboboxProps {
  options: ComboboxOption[];
  value?: string | undefined;
  onValueChange?: (value: string | undefined) => void;
  placeholder?: string;
  searchPlaceholder?: string;
  emptyText?: string;
  "aria-label"?: string;
  className?: string;
  disabled?: boolean;
}

/** Searchable single select (a Popover around Command). Selecting the current value clears it. */
export function Combobox({
  options,
  value,
  onValueChange,
  placeholder = "Select",
  searchPlaceholder = "Search",
  emptyText = "No results.",
  className,
  disabled,
  "aria-label": ariaLabel,
}: ComboboxProps) {
  const [open, setOpen] = useState(false);
  const current = options.find((o) => o.value === value);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        role="combobox"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={disabled}
        className={cn(
          "inline-flex h-7 w-56 items-center justify-between gap-2 rounded-md border border-border bg-bg-subtle px-2 text-sm text-fg",
          "hover:border-border-strong disabled:cursor-not-allowed disabled:opacity-50",
          focusRing,
          className,
        )}
      >
        <span className={cn("truncate", current ? "" : "text-fg-subtle")}>
          {current?.label ?? placeholder}
        </span>
        <ChevronsUpDown className="size-3.5 shrink-0 text-fg-muted" aria-hidden />
      </PopoverTrigger>
      <PopoverContent align="start" className="min-w-56 p-0">
        <Command>
          <CommandInput placeholder={searchPlaceholder} />
          <CommandList>
            <CommandEmpty>{emptyText}</CommandEmpty>
            {options.map((o) => (
              <CommandItem
                key={o.value}
                value={o.label}
                {...(o.keywords ? { keywords: o.keywords } : {})}
                {...(o.disabled ? { disabled: true } : {})}
                onSelect={() => {
                  onValueChange?.(o.value === value ? undefined : o.value);
                  setOpen(false);
                }}
              >
                <span className="truncate">{o.label}</span>
                {o.value === value ? (
                  <Check className="ml-auto size-3.5 text-accent" aria-hidden />
                ) : null}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
