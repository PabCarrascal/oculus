import type { KindRef, ResourceKind } from "@oculus/bindings";

/** "apps/v1/deployments"; el grupo core se escribe "core" para que la clave no empiece por "/". */
export function kindKey(k: KindRef): string {
  return `${k.group || "core"}/${k.version}/${k.plural}`;
}

export function parseKindKey(key: string): KindRef | null {
  const parts = key.split("/");
  if (parts.length !== 3 || parts.some((p) => !p)) return null;
  const [group, version, plural] = parts as [string, string, string];
  return { group: group === "core" ? "" : group, version, plural };
}

export function sameKind(a: KindRef, b: KindRef): boolean {
  return a.group === b.group && a.version === b.version && a.plural === b.plural;
}

/** Solo los tipos que se pueden listar y vigilar tienen tabla. */
export function watchable(k: ResourceKind): boolean {
  return k.verbs.includes("list") && k.verbs.includes("watch");
}
