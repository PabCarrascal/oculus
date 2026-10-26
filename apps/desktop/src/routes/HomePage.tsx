import type { ClusterSummary, KubeconfigSource } from "@oculus/bindings";
import { Button } from "@oculus/ui";
import { Link } from "@tanstack/react-router";
import { Boxes, Lock, Plus, ShieldAlert, SlidersHorizontal, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ClusterActions } from "../features/clusters/ClusterActions";
import { ClusterPrefsDialog } from "../features/clusters/ClusterPrefsDialog";
import { ClusterStatusLabel } from "../features/clusters/ClusterStatusLabel";
import { ErrorNotice } from "../features/clusters/ErrorNotice";
import { displayName } from "../features/clusters/naming";
import { useClusters, usePickSource, useRemoveSource, useSources } from "../stores/clusters";

export function HomePage() {
  const { t } = useTranslation();
  const sources = useSources();
  const clusters = useClusters();
  const pick = usePickSource();
  const remove = useRemoveSource();
  const list = clusters.data ?? [];

  return (
    <div className="mx-auto max-w-4xl px-8 py-6">
      <header className="flex items-center justify-between">
        <h1 className="text-lg font-semibold">{t("home.title")}</h1>
        <Button onClick={() => pick.mutate()} disabled={pick.isPending}>
          <Plus size={16} aria-hidden />
          {t("clusters.addSource")}
        </Button>
      </header>

      {pick.isError && (
        <div className="mt-4">
          <ErrorNotice error={pick.error} />
        </div>
      )}
      {remove.isError && (
        <div className="mt-4">
          <ErrorNotice error={remove.error} />
        </div>
      )}

      {sources.isSuccess &&
      clusters.isSuccess &&
      list.length === 0 &&
      sources.data.every((s) => !s.error) ? (
        <EmptyState />
      ) : (
        (sources.data ?? []).map((source) => (
          <SourceSection
            key={source.id}
            source={source}
            clusters={list.filter((c) => c.sourceId === source.id)}
            onRemove={() => remove.mutate(source.id)}
          />
        ))
      )}
    </div>
  );
}

function EmptyState() {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col items-center gap-3 py-24 text-center">
      <Boxes size={40} className="text-muted" aria-hidden />
      <h2 className="text-base font-medium">{t("home.emptyTitle")}</h2>
      <p className="max-w-sm text-sm text-muted">{t("home.emptyBody")}</p>
    </div>
  );
}

interface SourceSectionProps {
  source: KubeconfigSource;
  clusters: ClusterSummary[];
  onRemove: () => void;
}

function SourceSection({ source, clusters, onRemove }: SourceSectionProps) {
  const { t } = useTranslation();
  return (
    <section className="mt-6" aria-label={source.path}>
      <div className="flex items-center gap-2 text-xs text-muted">
        <span className="truncate font-mono" title={source.path}>
          {source.path}
        </span>
        <span className="shrink-0 rounded border border-border px-1.5 py-px">
          {t(`clusters.origin.${source.origin}`)}
        </span>
        <div className="flex-1" />
        {source.origin === "manual" && (
          <Button variant="ghost" className="h-6 px-2 text-xs" onClick={onRemove}>
            <Trash2 size={12} aria-hidden />
            {t("clusters.removeSource")}
          </Button>
        )}
      </div>
      {source.error ? (
        <p className="mt-2 text-sm text-error">
          {t("clusters.sourceError", { error: source.error })}
        </p>
      ) : clusters.length === 0 ? (
        <p className="mt-2 text-sm text-muted">{t("clusters.noContexts")}</p>
      ) : (
        <ul className="mt-2 divide-y divide-border overflow-hidden rounded-lg border border-border bg-surface">
          {clusters.map((c) => (
            <ClusterRow key={c.id} cluster={c} />
          ))}
        </ul>
      )}
    </section>
  );
}

function ClusterRow({ cluster }: { cluster: ClusterSummary }) {
  const { t } = useTranslation();
  const [prefsOpen, setPrefsOpen] = useState(false);
  const name = displayName(cluster);

  return (
    <li
      className="border-l-4 px-4 py-3"
      style={{ borderLeftColor: cluster.prefs.color ?? "transparent" }}
    >
      <div className="flex items-center gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <Link
              to="/clusters/$clusterId"
              params={{ clusterId: cluster.id }}
              className="max-w-full shrink-0 truncate font-medium text-text hover:underline focus-visible:outline-2 focus-visible:outline-focus"
            >
              {name}
            </Link>
            {cluster.prefs.alias && (
              <span className="min-w-0 truncate text-xs text-muted">{cluster.contextName}</span>
            )}
            {cluster.prefs.readOnly && (
              <Lock size={12} className="shrink-0 text-muted" aria-label={t("clusters.readOnly")} />
            )}
            {cluster.prefs.protected && (
              <ShieldAlert
                size={12}
                className="shrink-0 text-warning"
                aria-label={t("clusters.protected")}
              />
            )}
          </div>
          <div className="mt-0.5 flex min-w-0 items-center gap-3">
            <ClusterStatusLabel status={cluster.status} />
            {cluster.server && (
              <span className="min-w-0 truncate font-mono text-xs text-muted">
                {cluster.server}
              </span>
            )}
          </div>
        </div>
        <ClusterActions cluster={cluster} />
        <Button
          variant="ghost"
          className="px-2"
          aria-label={t("clusters.preferences")}
          title={t("clusters.preferences")}
          onClick={() => setPrefsOpen(true)}
        >
          <SlidersHorizontal size={16} aria-hidden />
        </Button>
      </div>
      {cluster.status.state === "error" && (
        <div className="mt-2">
          <ErrorNotice error={cluster.status.error} />
        </div>
      )}
      <ClusterPrefsDialog cluster={cluster} open={prefsOpen} onOpenChange={setPrefsOpen} />
    </li>
  );
}
