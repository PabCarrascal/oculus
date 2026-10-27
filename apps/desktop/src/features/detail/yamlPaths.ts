import { isMap, isScalar, LineCounter, parseDocument, stringify } from "yaml";

export interface Marker {
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  message: string;
}

/** JSON del backend → YAML para el editor, con el orden de claves original. */
export function toYaml(json: string): string {
  return stringify(JSON.parse(json), { lineWidth: 0, aliasDuplicateObjects: false });
}

export interface Parsed {
  /** JSON listo para enviar, o null si hay errores de sintaxis. */
  json: string | null;
  markers: Marker[];
}

/** YAML del editor → JSON. Los errores de sintaxis salen como marcadores con su línea. */
export function parseYaml(text: string): Parsed {
  const counter = new LineCounter();
  const doc = parseDocument(text, { lineCounter: counter, prettyErrors: false });
  if (doc.errors.length > 0) {
    return {
      json: null,
      markers: doc.errors.map((e) => {
        const start = counter.linePos(e.pos[0]);
        const end = counter.linePos(e.pos[1]);
        return {
          line: start.line,
          column: start.col,
          endLine: end.line,
          endColumn: end.col + 1,
          message: e.message,
        };
      }),
    };
  }
  const value = doc.toJS();
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return {
      json: null,
      markers: [
        {
          line: 1,
          column: 1,
          endLine: 1,
          endColumn: 2,
          message: "El documento debe ser un objeto",
        },
      ],
    };
  }
  return { json: JSON.stringify(value), markers: [] };
}

/** "spec.template.spec.containers[0].image" → ["spec", "template", "spec", "containers", 0, "image"] */
export function fieldToPath(field: string): (string | number)[] {
  const path: (string | number)[] = [];
  for (const part of field.split(".")) {
    const m = /^([^[\]]*)((?:\[\d+\])*)$/.exec(part);
    if (!m) {
      path.push(part);
      continue;
    }
    if (m[1]) path.push(m[1]);
    for (const idx of (m[2] ?? "").matchAll(/\[(\d+)\]/g)) path.push(Number(idx[1]));
  }
  return path;
}

/**
 * Posición de un campo que el API server rechazó. Marca la clave si existe; si
 * no (p. ej. un campo obligatorio que falta), el nodo más profundo que sí existe.
 */
export function locate(text: string, field: string, message: string): Marker {
  const counter = new LineCounter();
  const doc = parseDocument(text, { lineCounter: counter });
  const path = fieldToPath(field);
  for (let depth = path.length; depth > 0; depth--) {
    const parentPath = path.slice(0, depth - 1);
    const last = path[depth - 1];
    const parent = parentPath.length ? doc.getIn(parentPath, true) : doc.contents;
    let range: [number, number, number?] | null | undefined;
    if (isMap(parent)) {
      const pair = parent.items.find((p) => isScalar(p.key) && p.key.value === last);
      range = pair && isScalar(pair.key) ? pair.key.range : undefined;
    } else {
      const node = doc.getIn(path.slice(0, depth), true) as
        | { range?: [number, number, number] }
        | undefined;
      range = node?.range;
    }
    if (range) {
      const start = counter.linePos(range[0]);
      const end = counter.linePos(range[1]);
      return {
        line: start.line,
        column: start.col,
        endLine: end.line,
        endColumn: end.col,
        message,
      };
    }
  }
  return { line: 1, column: 1, endLine: 1, endColumn: 2, message };
}
