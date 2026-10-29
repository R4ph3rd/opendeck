import { Check } from "lucide-react";
import { ContextMenu as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn, menuContent, menuItem } from "../lib/cn";

export const ContextMenu = Primitive.Root;
export const ContextMenuTrigger = Primitive.Trigger;
export const ContextMenuGroup = Primitive.Group;

export function ContextMenuContent({
  className,
  ...props
}: ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Primitive.Content className={cn(menuContent, className)} {...props} />
    </Primitive.Portal>
  );
}

export function ContextMenuItem({ className, ...props }: ComponentProps<typeof Primitive.Item>) {
  return <Primitive.Item className={cn(menuItem, className)} {...props} />;
}

export function ContextMenuCheckboxItem({
  className,
  children,
  ...props
}: ComponentProps<typeof Primitive.CheckboxItem>) {
  return (
    <Primitive.CheckboxItem className={cn(menuItem, "pl-7", className)} {...props}>
      <span className="absolute left-2 flex size-4 items-center justify-center">
        <Primitive.ItemIndicator>
          <Check className="size-3.5" aria-hidden />
        </Primitive.ItemIndicator>
      </span>
      {children}
    </Primitive.CheckboxItem>
  );
}

export function ContextMenuLabel({ className, ...props }: ComponentProps<typeof Primitive.Label>) {
  return (
    <Primitive.Label
      className={cn("px-2 py-1 text-xs font-medium text-fg-subtle", className)}
      {...props}
    />
  );
}

export function ContextMenuSeparator({
  className,
  ...props
}: ComponentProps<typeof Primitive.Separator>) {
  return <Primitive.Separator className={cn("-mx-1 my-1 h-px bg-border", className)} {...props} />;
}

export function ContextMenuShortcut({ className, ...props }: ComponentProps<"span">) {
  return <span className={cn("ml-auto pl-4 text-xs text-fg-subtle", className)} {...props} />;
}
