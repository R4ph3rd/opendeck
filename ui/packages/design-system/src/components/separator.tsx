import { Separator as Primitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "../lib/cn";

export function Separator({
  className,
  orientation = "horizontal",
  decorative = true,
  ...props
}: ComponentProps<typeof Primitive.Root>) {
  return (
    <Primitive.Root
      orientation={orientation}
      decorative={decorative}
      className={cn(
        "shrink-0 bg-border",
        orientation === "horizontal" ? "h-px w-full" : "h-full w-px",
        className,
      )}
      {...props}
    />
  );
}
