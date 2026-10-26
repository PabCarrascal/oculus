export const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** Etiqueta del modificador principal para mostrar atajos. */
export const modKey = isMac ? "⌘" : "Ctrl";

/** ⌘ en macOS, Ctrl en el resto. */
export function hasMod(e: KeyboardEvent) {
  return isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}
