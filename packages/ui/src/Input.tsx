import type { InputHTMLAttributes } from "react";
import { cn } from "./cn";

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        "h-8 w-full rounded-md border border-border bg-bg px-2 text-sm text-text outline-none",
        "placeholder:text-muted focus-visible:border-focus",
        className,
      )}
      {...props}
    />
  );
}
