import type { CellValue, ColumnDef, Row } from "@oculus/bindings";
import { cn, StatusDot } from "@oculus/ui";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp } from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { formatAge, useNow } from "../../lib/time";
import type { Sort } from "./rows";

const ROW_HEIGHT = 32;

interface Props {
  columns: ColumnDef[];
  rows: Row[];
  sort: Sort;
  onSort: (sort: Sort) => void;
  selected: Set<string>;
  onSelect: (selected: Set<string>) => void;
  /** Fila abierta en el panel de detalle. */
  activeUid?: string;
  onOpen: (row: Row) => void;
}

/** Tabla virtualizada: solo existen en el DOM las filas visibles (D4). */
export function ResourceTable({
  columns,
  rows,
  sort,
  onSort,
  selected,
  onSelect,
  activeUid,
  onOpen,
}: Props) {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const now = useNow();
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    // Tamaño supuesto hasta la primera medida: así el primer frame ya trae filas.
    initialRect: { width: 1200, height: 800 },
  });

  const template = `36px ${columns.map((c) => width(c)).join(" ")}`;
  const allSelected = rows.length > 0 && rows.every((r) => selected.has(r.uid));
  const label = (c: ColumnDef) => (c.translate ? t(c.label as never) : c.label);

  const toggle = (uid: string) => {
    const next = new Set(selected);
    if (next.has(uid)) next.delete(uid);
    else next.add(uid);
    onSelect(next);
  };

  return (
    <div ref={scrollRef} className="min-h-0 flex-1 overflow-auto">
      {/* Tabla semántica con CSS grid: el patrón de TanStack Virtual para tablas virtualizadas. */}
      <table className="grid w-full min-w-full" aria-rowcount={rows.length + 1}>
        <thead className="sticky top-0 z-10 grid">
          <tr
            className="grid border-b border-border bg-surface text-xs font-medium text-muted"
            style={{ gridTemplateColumns: template, minWidth: "100%" }}
          >
            <th className="flex items-center justify-center">
              <input
                type="checkbox"
                aria-label={t("explorer.selectAll")}
                checked={allSelected}
                onChange={() => onSelect(allSelected ? new Set() : new Set(rows.map((r) => r.uid)))}
              />
            </th>
            {columns.map((c) => (
              <th
                key={c.id}
                className="font-medium"
                aria-sort={sort.column === c.id ? (sort.desc ? "descending" : "ascending") : "none"}
              >
                <button
                  type="button"
                  className={cn(
                    "flex h-8 w-full items-center gap-1 px-2 text-left hover:text-text",
                    c.type === "number" && "justify-end",
                    "focus-visible:outline-2 focus-visible:outline-focus",
                  )}
                  onClick={() =>
                    onSort({ column: c.id, desc: sort.column === c.id ? !sort.desc : false })
                  }
                >
                  <span className="truncate">{label(c)}</span>
                  {sort.column === c.id &&
                    (sort.desc ? (
                      <ArrowDown size={12} aria-hidden />
                    ) : (
                      <ArrowUp size={12} aria-hidden />
                    ))}
                </button>
              </th>
            ))}
          </tr>
        </thead>

        <tbody
          className="relative grid"
          style={{ height: virtualizer.getTotalSize(), minWidth: "100%" }}
        >
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index] as Row;
            const isSelected = selected.has(row.uid);
            return (
              <tr
                key={row.uid}
                aria-rowindex={item.index + 2}
                aria-selected={isSelected}
                className={cn(
                  "absolute left-0 grid w-full items-center border-b border-border/60 text-sm",
                  row.uid === activeUid
                    ? "bg-accent/15"
                    : isSelected
                      ? "bg-accent/10"
                      : "hover:bg-surface-2",
                )}
                style={{
                  height: ROW_HEIGHT,
                  transform: `translateY(${item.start}px)`,
                  gridTemplateColumns: template,
                }}
              >
                <td className="flex items-center justify-center">
                  <input
                    type="checkbox"
                    aria-label={row.name}
                    checked={isSelected}
                    onChange={() => toggle(row.uid)}
                  />
                </td>
                {columns.map((c) => (
                  <td
                    key={c.id}
                    className={cn(
                      "truncate px-2",
                      c.type === "number" && "text-right tabular-nums",
                    )}
                  >
                    <Cell row={row} column={c} now={now} onOpen={onOpen} />
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Cell({
  row,
  column,
  now,
  onOpen,
}: {
  row: Row;
  column: ColumnDef;
  now: number;
  onOpen: (row: Row) => void;
}) {
  switch (column.id) {
    case "name":
      return (
        <button
          type="button"
          onClick={() => onOpen(row)}
          className="max-w-full truncate text-left font-medium text-text hover:underline focus-visible:outline-2 focus-visible:outline-focus"
        >
          {row.name}
        </button>
      );
    case "namespace":
      return <span className="text-muted">{row.namespace}</span>;
    case "status":
      return row.status ? (
        <span className="inline-flex items-center gap-1.5">
          <StatusDot tone={row.status.tone} />
          {row.status.text}
        </span>
      ) : null;
    case "age":
      return (
        <span className="tabular-nums text-muted" title={row.createdAt ?? ""}>
          {formatAge(row.createdAt, now)}
        </span>
      );
  }
  return <Value value={row.cells[column.id]} type={column.type} now={now} />;
}

function Value({
  value,
  type,
  now,
}: {
  value: CellValue | undefined;
  type: ColumnDef["type"];
  now: number;
}) {
  if (value == null) return null;
  if (typeof value === "object") {
    const entries = Object.entries(value.labels);
    return (
      <span className="text-xs text-muted" title={entries.map(([k, v]) => `${k}=${v}`).join("\n")}>
        {entries
          .slice(0, 2)
          .map(([k, v]) => `${k}=${v}`)
          .join(", ")}
        {entries.length > 2 && ` +${entries.length - 2}`}
      </span>
    );
  }
  if (type === "date" && typeof value === "string") {
    return (
      <span className="tabular-nums text-muted" title={value}>
        {formatAge(value, now)}
      </span>
    );
  }
  return <span title={String(value)}>{String(value)}</span>;
}

function width(c: ColumnDef): string {
  switch (c.id) {
    case "name":
      return "minmax(220px, 3fr)";
    case "namespace":
      return "minmax(120px, 1fr)";
    case "status":
      return "minmax(140px, 1.2fr)";
    case "message":
      return "minmax(240px, 3fr)";
    case "age":
      return "72px";
  }
  return c.type === "number" ? "90px" : "minmax(110px, 1fr)";
}
