import { cn } from "./cn";

export type Tone = "ok" | "warning" | "error" | "progress" | "neutral";

const tones: Record<Tone, string> = {
  ok: "bg-ok",
  warning: "bg-warning",
  error: "bg-error",
  progress: "bg-accent animate-pulse",
  neutral: "bg-border",
};

export function StatusDot({ tone, className }: { tone: Tone; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("inline-block size-2 shrink-0 rounded-full", tones[tone], className)}
    />
  );
}
