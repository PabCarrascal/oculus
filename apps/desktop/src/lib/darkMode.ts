import { useSyncExternalStore } from "react";

const query =
  typeof window !== "undefined" ? window.matchMedia?.("(prefers-color-scheme: dark)") : undefined;

function isDark() {
  const forced = document.documentElement.dataset.theme;
  return forced ? forced === "dark" : Boolean(query?.matches);
}

function subscribe(cb: () => void) {
  query?.addEventListener("change", cb);
  const observer = new MutationObserver(cb);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  return () => {
    query?.removeEventListener("change", cb);
    observer.disconnect();
  };
}

/** Tema efectivo (ajuste de la app o el del sistema), para componentes que no usan CSS. */
export function useDarkMode(): boolean {
  return useSyncExternalStore(subscribe, isDark);
}
