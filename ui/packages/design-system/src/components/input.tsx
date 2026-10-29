import type { InputHTMLAttributes, Ref } from "react";
import { cn, focusRing } from "../lib/cn";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  ref?: Ref<HTMLInputElement>;
}

export function Input({ className, type = "text", ref, ...props }: InputProps) {
  return (
    <input
      ref={ref}
      type={type}
      className={cn(
        "h-7 w-full min-w-0 rounded-md border border-border bg-bg-subtle px-2 text-sm text-fg",
        "placeholder:text-fg-subtle",
        "transition-colors duration-[var(--duration-fast)] ease-standard",
        "hover:border-border-strong aria-invalid:border-danger",
        "disabled:cursor-not-allowed disabled:opacity-50",
        focusRing,
        className,
      )}
      {...props}
    />
  );
}
