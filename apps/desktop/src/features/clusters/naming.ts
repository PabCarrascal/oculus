import type { ClusterSummary } from "@oculus/bindings";

export function displayName(c: ClusterSummary) {
  return c.prefs.alias ?? c.contextName;
}

/** Dos letras para el avatar de la barra lateral: "prod-eu-west" → "PE". */
export function initials(c: ClusterSummary) {
  const words = displayName(c)
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean);
  const letters =
    words.length > 1 ? `${words[0]?.[0]}${words[1]?.[0]}` : (words[0] ?? "?").slice(0, 2);
  return letters.toUpperCase();
}

export const PRESET_COLORS = [
  "#2f6bd8",
  "#1f8a4c",
  "#b26a00",
  "#c8323b",
  "#8a4fd8",
  "#0f8f8f",
  "#c2417a",
  "#5d6673",
] as const;
