import type { KindRef, ResourceKind, Scope } from "@oculus/bindings";
import { useNavigate, useParams, useSearch } from "@tanstack/react-router";
import { Search } from "lucide-react";
import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ClusterActions } from "../features/clusters/ClusterActions";
import { ErrorNotice } from "../features/clusters/ErrorNotice";
import { displayName } from "../features/clusters/naming";
import { ColumnMenu } from "../features/explorer/ColumnMenu";
import { KindSidebar } from "../features/explorer/KindSidebar";
import { NamespacePicker } from "../features/explorer/NamespacePicker";
import { ResourceTable } from "../features/explorer/ResourceTable";
import { filterRows, parseQuery, sortRows } from "../features/explorer/rows";
import { kindKey, parseKindKey, sameKind, watchable } from "../lib/kinds";
import { useCluster, useDiscovery, useNamespaces } from "../stores/clusters";
import { useExplorerPrefs } from "../stores/explorer";
import { useResourceSubscription } from "../stores/resources";
import { useUi } from "../stores/ui";

const DEFAULT_KIND = "core/v1/pods";

export interface ExplorerSearch {
  kind?: string;
  ns?: string;
  q?: string;
}

export function ExplorerPage() {
  const { t } = useTranslation();
  const { clusterId } = useParams({ from: "/clusters/$clusterId/explore" });
  const search = useSearch({ from: "/clusters/$clusterId/explore" });
  const navigate = useNavigate({ from: "/clusters/$clusterId/explore" });
  const cluster = useCluster(clusterId);
  const connected = cluster?.status.state === "connected";
  const kinds = useDiscovery(clusterId, connected).data ?? [];
  const namespaces = useNamespaces(clusterId, connected).data ?? [];

  const requested = parseKindKey(search.kind ?? DEFAULT_KIND);
  const resource: ResourceKind | undefined =
    kinds.find((k) => requested != null && sameKind(k, requested) && watchable(k)) ??
    kinds.find(watchable);
  const kind: KindRef | null = resource
    ? { group: resource.group, version: resource.version, plural: resource.plural }
    : null;
  const selectedNs = search.ns ? search.ns.split(",").filter(Boolean) : [];
  const scope: Scope =
    resource?.namespaced && selectedNs.length > 0
      ? { kind: "namespaces", names: selectedNs }
      : { kind: "all" };

  const view = useResourceSubscription(clusterId, connected ? kind : null, scope);
  const key = kind ? kindKey(kind) : "";

  const [query, setQuery] = useState(search.q ?? "");
  const deferredQuery = useDeferredValue(query);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const searchRef = useRef<HTMLInputElement>(null);

  const toggled = useExplorerPrefs((s) => s.toggled[key]);
  const sort = useExplorerPrefs((s) => s.sort[key]) ?? { column: "name", desc: false };
  const toggleColumn = useExplorerPrefs((s) => s.toggleColumn);
  const setSort = useExplorerPrefs((s) => s.setSort);

  const visible = useMemo(() => {
    const flipped = new Set(toggled ?? []);
    return new Set(
      view.columns
        .filter((c) => c.id === "name" || c.defaultVisible !== flipped.has(c.id))
        .map((c) => c.id),
    );
  }, [view.columns, toggled]);
  const columns = view.columns.filter((c) => visible.has(c.id));
  const rows = useMemo(
    () => sortRows(filterRows(view.rows, parseQuery(deferredQuery)), sort, view.columns),
    [view.rows, deferredQuery, sort, view.columns],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: la selección es de un tipo y ámbito concretos.
  useEffect(() => setSelected(new Set()), [key, search.ns]);

  // Estado del watch en la barra inferior.
  const setActiveWatch = useUi((s) => s.setActiveWatch);
  useEffect(() => {
    setActiveWatch(kind ? { state: view.state, rows: view.rows.length } : null);
  }, [kind, view.state, view.rows.length, setActiveWatch]);
  useEffect(() => () => setActiveWatch(null), [setActiveWatch]);

  // "/" enfoca el filtro, como en el wireframe.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (e.key === "/" && !target?.closest("input, textarea, [contenteditable]")) {
        e.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!cluster) {
    return <p className="px-8 py-6 text-sm text-muted">{t("clusters.notFound")}</p>;
  }
  if (!connected) {
    return (
      <div className="mx-auto flex max-w-md flex-col items-center gap-4 px-8 py-24 text-center">
        <p className="text-sm text-muted">{t("clusters.notConnected")}</p>
        <ClusterActions cluster={cluster} />
        {cluster.status.state === "error" && <ErrorNotice error={cluster.status.error} />}
      </div>
    );
  }

  const update = (patch: Partial<ExplorerSearch>) =>
    void navigate({ search: (prev) => ({ ...prev, ...patch }), replace: true });

  return (
    <div className="flex h-full min-h-0">
      <KindSidebar kinds={kinds} active={kind} onSelect={(k) => update({ kind: kindKey(k) })} />
      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex flex-wrap items-center gap-3 border-b border-border px-4 py-2">
          <h1 className="text-base font-semibold">
            {resource?.kind}
            <span className="ml-2 text-sm font-normal text-muted">
              {view.loaded
                ? rows.length === view.rows.length
                  ? rows.length
                  : `${rows.length} / ${view.rows.length}`
                : ""}
            </span>
          </h1>
          <span className="truncate text-xs text-muted">{displayName(cluster)}</span>
          <div className="flex-1" />
          {resource?.namespaced && (
            <NamespacePicker
              available={namespaces}
              selected={selectedNs}
              onChange={(ns) => update({ ns: ns.length ? ns.join(",") : undefined })}
            />
          )}
          <label className="relative">
            <Search size={14} className="absolute top-2 left-2 text-muted" aria-hidden />
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onBlur={() => update({ q: query || undefined })}
              placeholder={t("explorer.filterPlaceholder")}
              aria-label={t("explorer.filterPlaceholder")}
              spellCheck={false}
              className="h-8 w-64 rounded-md border border-border bg-bg pr-2 pl-7 text-sm outline-none focus-visible:border-focus"
            />
          </label>
          <ColumnMenu
            columns={view.columns}
            visible={visible}
            onToggle={(id) => toggleColumn(key, id)}
          />
        </header>

        {view.error && (view.state === "error" || view.state === "backoff") && (
          <div className="px-4 pt-3">
            <ErrorNotice error={view.error} />
          </div>
        )}
        {selected.size > 0 && (
          <div className="border-b border-border bg-accent/5 px-4 py-1.5 text-xs text-muted">
            {t("explorer.selected", { count: selected.size })}
          </div>
        )}

        {!view.loaded ? (
          view.state !== "error" && (
            <p className="px-4 py-6 text-sm text-muted">{t("explorer.loading")}</p>
          )
        ) : rows.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted">
            {view.rows.length === 0
              ? t("explorer.empty", { kind: resource?.kind })
              : t("explorer.noMatches")}
          </p>
        ) : (
          <ResourceTable
            columns={columns}
            rows={rows}
            sort={sort}
            onSort={(s) => setSort(key, s)}
            selected={selected}
            onSelect={setSelected}
          />
        )}
      </section>
    </div>
  );
}
