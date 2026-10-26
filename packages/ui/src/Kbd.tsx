import type { ReactNode } from "react";

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-border bg-surface-2 px-1.5 py-px font-mono text-[11px] text-muted">
      {children}
    </kbd>
  );
}
