import { cva, type VariantProps } from "class-variance-authority";
import { AlertTriangle, CheckCircle2, Info, OctagonAlert } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "../lib/cn";

const callout = cva("flex gap-3 rounded-md border border-l-4 p-3 text-sm text-fg", {
  variants: {
    variant: {
      info: "border-border border-l-info bg-info-bg",
      success: "border-border border-l-success bg-success-bg",
      warning: "border-border border-l-warning bg-warning-bg",
      danger: "border-border border-l-danger bg-danger-bg",
    },
  },
  defaultVariants: { variant: "info" },
});

const ICON = {
  info: Info,
  success: CheckCircle2,
  warning: AlertTriangle,
  danger: OctagonAlert,
} as const;

const TITLE = {
  info: "text-info",
  success: "text-success",
  warning: "text-warning",
  danger: "text-danger",
} as const;

export interface CalloutProps
  extends Omit<ComponentProps<"div">, "title">,
    VariantProps<typeof callout> {
  title?: ReactNode;
}

/** Inline message. Danger announces as an alert, the rest as polite status. */
export function Callout({ variant = "info", title, children, className, ...props }: CalloutProps) {
  const v = variant ?? "info";
  const Icon = ICON[v];
  return (
    <div
      role={v === "danger" ? "alert" : "status"}
      className={cn(callout({ variant: v }), className)}
      {...props}
    >
      <Icon aria-hidden className={cn("mt-0.5 size-4 shrink-0", TITLE[v])} />
      <div className="min-w-0 space-y-1">
        {title ? <p className={cn("font-semibold", TITLE[v])}>{title}</p> : null}
        <div className="text-fg-muted">{children}</div>
      </div>
    </div>
  );
}
