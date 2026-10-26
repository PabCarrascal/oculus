import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost";
}

const variants = {
  primary: "bg-accent text-accent-text hover:opacity-90",
  secondary: "bg-surface-2 text-text border border-border hover:bg-surface",
  ghost: "text-muted hover:text-text hover:bg-surface-2",
} as const;

export function Button({
  variant = "secondary",
  className,
  type = "button",
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex h-8 items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
        "disabled:pointer-events-none disabled:opacity-50",
        variants[variant],
        className,
      )}
      {...props}
    />
  );
}
