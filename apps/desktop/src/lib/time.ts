import { useSyncExternalStore } from "react";

/** Edad compacta como kubectl: 45s, 12m, 5h, 3d, 2y. */
export function formatAge(iso: string | null | undefined, now: number): string {
  if (!iso) return "";
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "";
  const s = Math.max(0, Math.floor((now - then) / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86_400) return `${Math.floor(s / 3600)}h`;
  if (s < 365 * 86_400) return `${Math.floor(s / 86_400)}d`;
  return `${Math.floor(s / (365 * 86_400))}y`;
}

// Un único reloj para toda la app: las edades se recalculan al pintar, nunca se guardan.
const listeners = new Set<() => void>();
let now = Date.now();
let timer: ReturnType<typeof setInterval> | undefined;

function subscribe(cb: () => void) {
  listeners.add(cb);
  timer ??= setInterval(() => {
    now = Date.now();
    for (const l of listeners) l();
  }, 10_000);
  return () => {
    listeners.delete(cb);
    if (listeners.size === 0 && timer) {
      clearInterval(timer);
      timer = undefined;
    }
  };
}

export function useNow(): number {
  return useSyncExternalStore(subscribe, () => now);
}
