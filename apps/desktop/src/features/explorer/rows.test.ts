import type { ColumnDef, Row } from "@oculus/bindings";
import { describe, expect, it } from "vitest";
import { filterRows, parseQuery, sortRows } from "./rows";

function row(name: string, over: Partial<Row> = {}): Row {
  return {
    uid: name,
    name,
    namespace: "default",
    createdAt: null,
    status: null,
    cells: {},
    ...over,
  };
}

const cols: ColumnDef[] = [
  {
    id: "restarts",
    label: "columns.restarts",
    translate: true,
    type: "number",
    defaultVisible: true,
  },
];

describe("filtro", () => {
  it("separa texto libre y labels", () => {
    expect(parseQuery("  Web  label:app=api label:tier ")).toEqual({
      text: "web",
      labels: [
        ["app", "api"],
        ["tier", null],
      ],
    });
  });

  it("filtra por nombre, namespace y labels", () => {
    const rows = [
      row("web-1", { cells: { labels: { labels: { app: "web", tier: "front" } } } }),
      row("api-1", { namespace: "payments", cells: { labels: { labels: { app: "api" } } } }),
      row("db-1"),
    ];
    const names = (q: string) => filterRows(rows, parseQuery(q)).map((r) => r.name);
    expect(names("")).toEqual(["web-1", "api-1", "db-1"]);
    expect(names("pay")).toEqual(["api-1"]);
    expect(names("label:app=web")).toEqual(["web-1"]);
    expect(names("label:tier")).toEqual(["web-1"]);
    expect(names("label:app")).toEqual(["web-1", "api-1"]);
    expect(names("1 label:app=api")).toEqual(["api-1"]);
  });
});

describe("orden", () => {
  it("ordena nombres con números como un humano", () => {
    const rows = [row("pod-10"), row("pod-2"), row("pod-1")];
    expect(sortRows(rows, { column: "name", desc: false }, cols).map((r) => r.name)).toEqual([
      "pod-1",
      "pod-2",
      "pod-10",
    ]);
  });

  it("los números se comparan como números y los vacíos van al final", () => {
    const rows = [
      row("a", { cells: { restarts: 10 } }),
      row("b"),
      row("c", { cells: { restarts: 2 } }),
    ];
    expect(sortRows(rows, { column: "restarts", desc: false }, cols).map((r) => r.name)).toEqual([
      "c",
      "a",
      "b",
    ]);
    expect(sortRows(rows, { column: "restarts", desc: true }, cols).map((r) => r.name)).toEqual([
      "b",
      "a",
      "c",
    ]);
  });

  it("la edad ascendente pone primero lo más reciente", () => {
    const rows = [
      row("viejo", { createdAt: "2026-01-01T00:00:00Z" }),
      row("nuevo", { createdAt: "2026-10-01T00:00:00Z" }),
    ];
    expect(sortRows(rows, { column: "age", desc: false }, cols).map((r) => r.name)).toEqual([
      "nuevo",
      "viejo",
    ]);
  });

  it("una columna desconocida (de otro tipo) cae al nombre", () => {
    const rows = [row("b"), row("a")];
    expect(
      sortRows(rows, { column: "nodo-que-no-existe", desc: false }, cols).map((r) => r.name),
    ).toEqual(["a", "b"]);
  });
});
