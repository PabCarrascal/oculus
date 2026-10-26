import type { Category, ResourceKind } from "@oculus/bindings";
import { Link, useParams } from "@tanstack/react-router";
import { Table2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ClusterActions } from "../features/clusters/ClusterActions";
import { ClusterStatusLabel } from "../features/clusters/ClusterStatusLabel";
import { ErrorNotice } from "../features/clusters/ErrorNotice";
import { displayName } from "../features/clusters/naming";
import { kindKey } from "../lib/kinds";
import { useCluster, useDiscovery, useNamespaces } from "../stores/clusters";

const CATEGORIES: Category[] = [
  "workloads",
  "config",
  "network",
  "storage",
  "access",
  "cluster",
  "custom",
];

export function ClusterPage() {
  const { t } = useTranslation();
  const { clusterId } = useParams({ from: "/clusters/$clusterId" });
  const cluster = useCluster(clusterId);
  const connected = cluster?.status.state === "connected";
  const discovery = useDiscovery(clusterId, connected);
  const namespaces = useNamespaces(clusterId, connected);

  if (!cluster) {
    return <p className="px-8 py-6 text-sm text-muted">{t("clusters.notFound")}</p>;
  }

  return (
    <div className="mx-auto max-w-4xl px-8 py-6">
      <header
        className="flex items-center gap-4 border-l-4 pl-3"
        style={{ borderLeftColor: cluster.prefs.color ?? "var(--color-border)" }}
      >
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-lg font-semibold">{displayName(cluster)}</h1>
          <ClusterStatusLabel status={cluster.status} />
        </div>
        {connected && (
          <Link
            to="/clusters/$clusterId/explore"
            params={{ clusterId }}
            className="inline-flex h-8 items-center gap-2 rounded-md bg-accent px-3 text-sm font-medium text-accent-text hover:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
          >
            <Table2 size={16} aria-hidden />
            {t("explorer.open")}
          </Link>
        )}
        <ClusterActions cluster={cluster} />
      </header>

      {cluster.status.state === "error" && (
        <div className="mt-4">
          <ErrorNotice error={cluster.status.error} />
        </div>
      )}

      <dl className="mt-6 grid grid-cols-[max-content_1fr] gap-x-8 gap-y-2 text-sm">
        <dt className="text-muted">{t("clusters.server")}</dt>
        <dd className="truncate font-mono text-xs leading-5">{cluster.server ?? "—"}</dd>
        <dt className="text-muted">{t("clusters.defaultNamespace")}</dt>
        <dd>{cluster.defaultNamespace ?? "default"}</dd>
        {cluster.status.state === "connected" && (
          <>
            <dt className="text-muted">{t("clusters.version")}</dt>
            <dd>{cluster.status.serverVersion}</dd>
          </>
        )}
      </dl>

      {!connected ? (
        cluster.status.state !== "error" && (
          <p className="mt-8 text-sm text-muted">{t("clusters.notConnected")}</p>
        )
      ) : (
        <>
          <section className="mt-8">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">
              {t("clusters.namespaces")} {namespaces.data && `(${namespaces.data.length})`}
            </h2>
            {namespaces.isError && <ErrorNotice error={namespaces.error} />}
            <ul className="mt-2 flex flex-wrap gap-1.5">
              {namespaces.data?.map((ns) => (
                <li
                  key={ns}
                  className="rounded border border-border bg-surface px-2 py-0.5 font-mono text-xs"
                >
                  {ns}
                </li>
              ))}
            </ul>
          </section>
          <section className="mt-8">
            <h2 className="text-xs font-semibold uppercase tracking-wide text-muted">
              {t("clusters.resources")}{" "}
              {discovery.data &&
                `(${t("clusters.resourcesCount", { count: discovery.data.length })})`}
            </h2>
            <div className="mt-2 grid gap-3 sm:grid-cols-2">
              {CATEGORIES.map((category) => (
                <CategoryCard
                  key={category}
                  clusterId={clusterId}
                  category={category}
                  kinds={(discovery.data ?? []).filter((k) => k.category === category)}
                />
              ))}
            </div>
          </section>
        </>
      )}
    </div>
  );
}

function CategoryCard({
  clusterId,
  category,
  kinds,
}: {
  clusterId: string;
  category: Category;
  kinds: ResourceKind[];
}) {
  const { t } = useTranslation();
  if (kinds.length === 0) return null;
  return (
    <details
      className="rounded-lg border border-border bg-surface px-3 py-2"
      open={category !== "custom"}
    >
      <summary className="cursor-default text-sm font-medium">
        {t(`clusters.category.${category}`)} <span className="text-muted">({kinds.length})</span>
      </summary>
      <ul className="mt-2 space-y-0.5 text-xs text-muted">
        {kinds.map((k) => (
          <li key={`${k.group}/${k.kind}`} className="flex justify-between gap-2">
            <Link
              to="/clusters/$clusterId/explore"
              params={{ clusterId }}
              search={{ kind: kindKey(k) }}
              className="text-text hover:underline"
            >
              {k.kind}
            </Link>
            <span className="truncate font-mono">
              {k.group ? `${k.group}/${k.version}` : k.version}
            </span>
          </li>
        ))}
      </ul>
    </details>
  );
}
