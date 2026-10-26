import type { Theme } from "@oculus/bindings";

/** "system" quita el atributo y deja que mande `prefers-color-scheme` (ver theme.css). */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement) {
  if (theme === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.dataset.theme = theme;
  }
}
