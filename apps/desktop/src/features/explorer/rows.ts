import type { CellValue, ColumnDef, Row } from "@oculus/bindings";

export interface Query {
  text: string;
  labels: [string, string | null][];
}

/** Texto libre (nombre o namespace) y `label:clave=valor` o `label:clave`. */
export function parseQuery(input: string): Query {
  const labels: [string, string | null][] = [];
  const text: string[] = [];
  for (const token of input.trim().split(/\s+/).filter(Boolean)) {
    const m = /^label:([^=]+)(?:=(.*))?$/i.exec(token);
    if (m?.[1]) labels.push([m[1], m[2] ?? null]);
    else text.push(token.toLowerCase());
  }
  return { text: text.join(" "), labels };
}

export function filterRows(rows: Row[], q: Query): Row[] {
  if (!q.text && q.labels.length === 0) return rows;
  return rows.filter((r) => {
    if (q.text && !`${r.namespace ?? ""}/${r.name}`.toLowerCase().includes(q.text)) return false;
    if (q.labels.length === 0) return true;
    const cell = r.cells.labels;
    const labels = cell && typeof cell === "object" && cell !== null ? cell.labels : {};
    return q.labels.every(([k, v]) => k in labels && (v === null || labels[k] === v));
  });
}

export interface Sort {
  column: string;
  desc: boolean;
}

export function cellValue(row: Row, columnId: string): CellValue | undefined {
  switch (columnId) {
    case "name":
      return row.name;
    case "namespace":
      return row.namespace ?? "";
    case "status":
      return row.status?.text ?? "";
    case "age":
      return row.createdAt ?? "";
    default:
      return row.cells[columnId];
  }
}

const FIXED = new Set(["name", "namespace", "status", "age"]);

type Key = string | number | null;

/**
 * Claves de orden cacheadas por objeto `Row`. Con 500 cambios/s solo cambian
 * unas decenas de filas por lote: casi todas las claves salen de la caché y la
 * comparación es un `<` entre primitivos, no un `Intl.Collator` (D4, etapa 2).
 */
const keyCache = new WeakMap<Row, Map<string, Key>>();

function sortKey(row: Row, column: string): Key {
  let keys = keyCache.get(row);
  if (!keys) {
    keys = new Map();
    keyCache.set(row, keys);
  }
  let key = keys.get(column);
  if (key === undefined) {
    const v = cellValue(row, column);
    key = v == null || typeof v === "object" ? null : typeof v === "number" ? v : natural(v);
    keys.set(column, key);
  }
  return key;
}

/** "Pod-10" → "pod-000000000010": el orden lexicográfico queda como el humano. */
function natural(s: string): string {
  return s.toLowerCase().replace(/\d+/g, (d) => d.padStart(12, "0"));
}

function compareKeys(a: Key, b: Key): number {
  if (a === b) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  const sa = String(a);
  const sb = String(b);
  return sa < sb ? -1 : sa > sb ? 1 : 0;
}

/**
 * Orden estable con desempate por namespace y nombre. La edad se ordena por
 * fecha de creación al revés: "más reciente" primero en ascendente, como kubectl.
 */
export function sortRows(rows: Row[], sort: Sort, columns: ColumnDef[]): Row[] {
  const isAge = sort.column === "age";
  // Las columnas fijas existen en todos los tipos aunque no vengan en `columns`.
  const known = FIXED.has(sort.column) || columns.some((c) => c.id === sort.column);
  const column = known ? sort.column : "name";
  const dir = sort.desc ? -1 : 1;
  return [...rows].sort((x, y) => {
    let c = compareKeys(sortKey(x, column), sortKey(y, column));
    if (isAge) c = -c;
    if (c === 0) {
      c =
        compareKeys(sortKey(x, "namespace"), sortKey(y, "namespace")) ||
        compareKeys(sortKey(x, "name"), sortKey(y, "name"));
    }
    return c * dir;
  });
}
